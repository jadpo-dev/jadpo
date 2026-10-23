"""Shared executable golden todo contract. No timing or implementation scores."""
from __future__ import annotations
import datetime as dt
import json
import re
import uuid
from collections import Counter
from urllib.parse import quote

NOW = '2026-01-15T12:00:00Z'
MILLIS = 1768478400000
ALICE = '00000000-0000-4000-8000-000000000001'
BOB = '00000000-0000-4000-8000-000000000002'
SERVICE = '00000000-0000-4000-8000-000000000003'
T1 = '00000000-0000-4000-8000-000000000011'
T2 = '00000000-0000-4000-8000-000000000012'
MISSING = '00000000-0000-4000-8000-000000000099'
PUBLIC_FIELDS = {'id','title','status','due_at','created_at','updated_at'}
CASES = {}
def case(identifier):
    def register(function):
        if identifier in CASES: raise ValueError(f'duplicate case {identifier}')
        CASES[identifier] = function
        return function
    return register

def require(condition, message):
    if not condition: raise AssertionError(message)

def iso(milliseconds):
    return dt.datetime.fromtimestamp(milliseconds/1000,dt.timezone.utc).isoformat(timespec='milliseconds').replace('+00:00','Z')

def counts(statements):
    result = Counter(principal=0, credential=0, business=0, runtime=0, transaction=0)
    for sql in statements:
        text = sql.upper()
        if text in ('COMMIT','ROLLBACK') or text.startswith(('BEGIN','SAVEPOINT','RELEASE SAVEPOINT','ROLLBACK TO SAVEPOINT')):
            result['transaction'] += 1
        elif ('"JADPO_SESSIONS"' in text or '"JADPO_SERVICE_KEYS"' in text) and ('"USER"' in text or '"SERVICE"' in text):
            result['principal'] += 1
        elif re.search(r'WHERE.*\"AUTHENTICATION_SUBJECT\"',text) and text.startswith('SELECT'):
            result['principal'] += 1
        elif '"JADPO_SESSIONS"' in text or '"JADPO_SERVICE_KEYS"' in text:
            result['credential'] += 1
        elif any(f'"{table}"' in text for table in ('TODO','USER','SERVICE')):
            result['business'] += 1
        else:
            result['runtime'] += 1
    require(sum(result.values()) == len(statements),'each observed statement is classified once')
    return dict(result)

class Contract:
    """One implementation-neutral controller; driver owns transport and fixture host."""
    def __init__(self, driver): self.driver=driver; self.auth={}; self.history=[]
    def call(self, op, **arguments):
        answer=self.driver.call({'op':op,**arguments})
        self.history.append({'operation':op,'statements':answer.get('statements',[]),'counts':counts(answer.get('statements',[])),'transaction_begins':answer.get('transaction_begins',0)})
        return answer['result']
    def sql(self, statement):
        result=self.call('sql',sql=statement)
        require('error' not in result,f'fixture statement refused: {result.get("error")}')
        return result['rows']
    def scalar(self, statement): return self.sql(statement)[0][0]
    def reset(self):
        self.call('outage',database=False,issuer=False)
        self.call('clock',at=MILLIS)
        self.call('worker_reset')
        self.call('mail',answers=[],clear=True)
        self.sql('DO $$ DECLARE r record; BEGIN FOR r IN SELECT tablename FROM pg_tables WHERE schemaname=\'public\' AND tablename<>\'jadpo_migrations\' LOOP EXECUTE format(\'TRUNCATE TABLE %I CASCADE\',r.tablename); END LOOP; END $$')
        self.sql(f'''INSERT INTO "user"(id,authentication_subject,status,email) VALUES ('{ALICE}','alice','active','alice@example.test'),('{BOB}','bob','active','bob@example.test')''')
        self.sql(f'''INSERT INTO service(id,owner_id,name,status) VALUES ('{SERVICE}','{ALICE}','reporting','active')''')
        self.auth={}
        for who,identifier in [('alice',ALICE),('bob',BOB)]:
            for method in ['session','bounded-bearer']:
                secret=uuid.uuid4().hex+uuid.uuid4().hex
                csrf=uuid.uuid4().hex+uuid.uuid4().hex
                prefix='' if method=='session' else 'jadpo_token_'
                verifier=self.call('digest',text=prefix+secret)['digest']
                csrf_hash=self.call('digest',text=csrf)['digest'] if method=='session' else None
                row=str(uuid.uuid4())
                self.sql(f'''INSERT INTO jadpo_sessions(id,principal_id,verifier,csrf_verifier,started_at,ends_at) VALUES ('{row}','{identifier}','{verifier}',{repr(csrf_hash) if csrf_hash else 'NULL'},'2026-01-15T11:59:00Z','2026-02-14T11:59:00Z')''')
                payload={'enc':1,'m':'browser_session' if method=='session' else 'api_token','r':uuid.UUID(row).hex,'v':verifier,'p':uuid.UUID(identifier).hex,'s':'t'+who,'e':MILLIS+240000}
                if csrf_hash:payload['c']=csrf_hash
                signed=self.call('sign',payload=json.dumps(payload,separators=(',',':')))['signed']
                credential=prefix+secret+'.'+signed
                self.auth[f'{who}-{method}']=[['cookie','todo_session='+credential],['origin','https://todo.example.test'],['x-jadpo-csrf',csrf]] if method=='session' else [['authorization','Bearer '+credential]]
            self.auth[f'{who}-jwt']=[['authorization','Bearer '+self.call('jwt',subject=who)['token']]]
        service_key='jadpo_key_'+uuid.uuid4().hex+uuid.uuid4().hex
        verifier=self.call('digest',text=service_key)['digest']
        self.sql(f'''INSERT INTO jadpo_service_keys(id,principal_id,verifier,started_at,ends_at) VALUES ('{uuid.uuid4()}','{SERVICE}','{verifier}','2026-01-15T11:59:00Z','2026-02-14T11:59:00Z')''')
        self.auth['reporting-service-key']=[['authorization','Bearer '+service_key]]
        self.auth['invalid-jwt']=[['authorization','Bearer invalid.jwt.signature']]
        self.auth['invalid-bearer']=[['authorization','Bearer invalid']]
        self.history=[]
    def todo(self, identifier=T1, owner=ALICE, title='Original', status='open', due='2026-02-01T00:00:00Z', sent=None, deleted=None, created=NOW, revision=1):
        def value(item):return 'NULL' if item is None else "'"+str(item).replace("'","''")+"'"
        self.sql(f'''INSERT INTO todo(id,owner_id,title,status,due_at,reminder_sent_at,reminder_queued_at,reminder_revision,deleted_at,created_at,updated_at) VALUES ('{identifier}','{owner}',{value(title)},'{status}',{value(due)},{value(sent)},NULL,{revision},{value(deleted)},{value(created)},{value(created)})''')
    def row(self,identifier=T1):
        rows=self.sql(f'''SELECT row_to_json(t)::text FROM todo t WHERE id='{identifier}' ''')
        return json.loads(rows[0][0]) if rows else None
    def request(self,method,path,auth='alice-session',body=None,raw=None):
        headers=[]
        for item in ([auth] if isinstance(auth,str) else auth or []):headers.extend(self.auth[item])
        if body is not None or raw is not None:headers.append(['content-type','application/json'])
        response=self.call('request',method=method,path=path,headers=headers,body=raw if raw is not None else json.dumps(body) if body is not None else '')
        response['json']=json.loads(response['body']) if response['body'] else None
        response['counts']=self.history[-1]['counts']
        return response
    def success(self,response,status=200):
        require(response['status']==status,f'expected {status}, got {response}')
        return response['json']
    def failure(self,response,status,code):
        require(response['status']==status,f'expected {status}, got {response}')
        body=response['json'];require(set(body)=={'error'},'failure has exactly one outer error object')
        error=body['error'];require({'code','message','request_id'}<=set(error)<= {'code','message','request_id','details'},'safe error fields')
        require(error['code']==code,f'expected {code}, got {error}')
        require(isinstance(error['message'],str) and bool(error['request_id']),'failure message and request id present')
        headers={k.lower():v for k,v in response['headers']};require(headers.get('x-request-id')==error['request_id'],'nested and header request id agree')
        for forbidden in ['SELECT ','INSERT ','Traceback','private-provider-secret','password@']:
            require(forbidden not in response['body'],'internal data disclosed')
        return error
    def view(self,item):
        require(set(item)==PUBLIC_FIELDS,f'exact public todo projection: {set(item)}')
        uuid.UUID(item['id']);require(item['status'] in ['open','done'],'closed status')
        return item
    def no_queries(self,response):
        require(sum(response['counts'][k] for k in ['principal','credential','business','runtime'])==0,f'no physical queries: {response["counts"]}')
    def unchanged(self,before,identifier=T1):require(self.row(identifier)==before,'persisted todo changed on refusal/no-op')
    def job(self):
        self.call('clock',at=MILLIS-60000);self.call('tick');self.call('clock',at=MILLIS);return self.call('tick')
    def mail(self):return self.call('mail')

@case('AUTH-001')
def auth001(c):
    r=c.request('GET','/todos',auth=None);c.failure(r,401,'authentication_required');c.no_queries(r)
@case('AUTH-002')
def auth002(c):
    c.todo();r=c.request('GET','/todos');items=c.success(r)['items'];require([t['id'] for t in items]==[T1],'session resolves Alice');c.view(items[0])
@case('AUTH-003')
def auth003(c):
    c.todo();c.todo(T2,owner=BOB);r=c.request('GET','/todos',auth='alice-jwt');items=c.success(r)['items'];require([t['id'] for t in items]==[T1],'JWT subject resolves current Alice');require(r['counts']['principal']==1,'JWT current authority query');c.view(items[0])
@case('AUTH-004')
def auth004(c):
    r=c.request('GET','/todos',auth=['alice-session','bob-jwt']);c.failure(r,401,'ambiguous_credentials');c.no_queries(r)
@case('AUTH-005')
def auth005(c):
    r=c.request('GET','/todos',auth=['alice-session','invalid-jwt']);c.failure(r,401,'ambiguous_credentials');c.no_queries(r)
@case('AUTH-006')
def auth006(c):
    c.sql(f'''UPDATE "user" SET status='disabled' WHERE id='{ALICE}' ''');r=c.request('DELETE','/users/'+ALICE);c.failure(r,403,'user_disabled');require(r['counts']['business']==0,'no app mutation after fresh refusal')
@case('AUTH-007')
def auth007(c):
    c.todo();r=c.request('GET',f'/users/{ALICE}/todos',auth='alice-jwt');out=c.success(r);require(set(out)=={'id','todos'},'private email absent');require('attacker' not in r['body'] and 'alice@example' not in r['body'],'claim/profile email absent')
@case('AUTH-008')
def auth008(c):
    c.todo();r=c.request('GET','/todos',auth='alice-bounded-bearer');require(c.success(r)['items'][0]['id']==T1,'bounded bearer Alice');require(r['counts']['principal']==r['counts']['credential']==0,'bounded bearer performs no authority/cache read')
@case('AUTH-009')
def auth009(c):
    r=c.request('POST','/auth/exchange',auth='reporting-service-key');out=c.success(r);require(set(out)=={'token','expires_at'} and out['token'].startswith('jadpo_service_token_'),'distinct service exchange');require(r['counts']['principal']==1,'one joined service authority query');verified=c.call('open',signed=out['token'].removeprefix('jadpo_service_token_'))
    claims=json.loads(verified['payload']);require(claims['p']==uuid.UUID(SERVICE).hex and claims['s']=='treporting','verified exchanged reporting identity')
    c.auth['reporting-exchanged']=[['authorization','Bearer '+out['token']]]
    c.failure(c.request('POST','/todos',auth='reporting-exchanged',body={'title':'Service cannot be user'}),403,'not_permitted');require('jadpo_key_' not in r['body'],'raw service key absent')
@case('AUTH-010')
def auth010(c):
    r=c.request('DELETE','/users/'+ALICE,auth='alice-bounded-bearer');c.success(r,204);require(r['counts']['principal']==1,'exactly one physical fresh principal query');require(c.scalar(f'''SELECT status FROM "user" WHERE id='{ALICE}' ''')=='disabled','persisted user disabled')
@case('AUTH-011')
def auth011(c):
    c.sql(f'''UPDATE service SET status='disabled' WHERE id='{SERVICE}' ''');r=c.request('POST','/auth/exchange',auth='reporting-service-key');c.failure(r,403,'service_disabled')
@case('AUTH-012')
def auth012(c):
    r=c.request('GET','/health/live',auth='invalid-bearer');require(c.success(r)=={'status':'live'},'runtime live shape');c.no_queries(r)
@case('PUBLIC-001')
def public001(c):
    r=c.request('GET','/health/live',auth=None);require(c.success(r)=={'status':'live'},'public runtime liveness');c.no_queries(r)
    for method,path in [('GET','/todos'),('POST','/todos'),('GET','/todos/'+T1),('PATCH','/todos/'+T1),('DELETE','/todos/'+T1),('GET',f'/users/{ALICE}/todos'),('DELETE','/users/'+ALICE)]:c.failure(c.request(method,path,auth=None),401,'authentication_required')
@case('CREATE-001')
def create001(c):
    item=c.view(c.success(c.request('POST','/todos',body={'title':'File accounts','due_at':'2026-01-20T09:00:00Z'}),201));row=c.row(item['id']);require(row['owner_id']==ALICE,'owner comes from principal');require(row['title']=='File accounts' and row['due_at'].startswith('2026-01-20T09:00:00'),'persisted requested values')
@case('CREATE-002')
def create002(c):
    c.failure(c.request('POST','/todos',body={'title':'Injected','due_at':None,'owner_id':BOB,'deleted_at':None}),422,'invalid_input');require(c.scalar('SELECT count(*) FROM todo')==0,'no injected row')
@case('CREATE-003')
def create003(c):
    c.failure(c.request('POST','/todos',body={'title':'Late','due_at':'2026-01-15T11:59:59Z'}),422,'due_date_in_past');require(c.scalar('SELECT count(*) FROM todo')==0,'past due write rolled back')
@case('CREATE-004')
def create004(c):
    item=c.success(c.request('POST','/todos',body={'title':'No deadline'}),201);require(item['due_at'] is None and c.row(item['id'])['due_at'] is None,'omitted due_at is persisted none')
@case('READ-001')
def read001(c):
    c.todo();item=c.view(c.success(c.request('GET','/todos/'+T1)));require(item['id']==T1,'owner reads todo')
@case('READ-002')
def read002(c):
    c.todo(T2,owner=BOB);responses=[c.request('GET','/todos/'+i) for i in [T2,MISSING]];errors=[c.failure(r,404,'todo_not_found') for r in responses];require([{k:v for k,v in e.items() if k!='request_id'} for e in errors][0]=={k:v for k,v in errors[1].items() if k!='request_id'},'concealed same body except request id')
@case('READ-003')
def read003(c):
    r=c.request('GET','/todos/not-a-uuid');c.failure(r,422,'invalid_input');c.no_queries(r)
@case('LIST-001')
def list001(c):
    for i in range(5):c.todo(str(uuid.UUID(int=0x40008000000000000100+i)),created=NOW,title=f'item {i}')
    c.todo(T2,owner=BOB);c.todo(T1,deleted=NOW);r=c.request('GET','/todos?page_size=2');page=c.success(r);require(len(page['items'])==2 and page['next'],'bounded next page');ids=[i['id'] for i in page['items']];require(ids==sorted(ids,reverse=True),'equal-time id-desc tie break');require(T1 not in ids and T2 not in ids,'policy/lifecycle before limit');require(r['counts']['business']==1,'one physical business list statement')
@case('LIST-002')
def list002(c):
    expected=[]
    for i in range(5):identifier=str(uuid.UUID(int=0x40008000000000000100+i));expected.append(identifier);c.todo(identifier)
    def walk():
        found=[];after=None
        for _ in range(4):
            page=c.success(c.request('GET','/todos?page_size=2'+('&after='+quote(after,safe='') if after else '')));found.extend(x['id'] for x in page['items']);after=page['next']
            if after is None:break
        require(after is None,'pagination terminates');return found
    first=walk();require(first==sorted(expected,reverse=True) and len(set(first))==5,'no gaps or duplicates');require(walk()==first,'repeat stable')
@case('LIST-003')
def list003(c):
    c.todo(due='2026-01-20T00:00:00Z');c.todo(T2,owner=BOB,due='2026-01-20T00:00:00Z');c.todo(MISSING,due=None);c.todo(str(uuid.uuid4()),status='done',due='2026-01-20T00:00:00Z');c.todo(str(uuid.uuid4()),due='2026-03-01T00:00:00Z');items=c.success(c.request('GET','/todos?status=open&due_before=2026-02-01T00%3A00%3A00Z'))['items'];require([i['id'] for i in items]==[T1],'filters compose with owner and null exclusion')
@case('LIST-004')
def list004(c):
    r=c.request('GET','/todos?page_size=101');c.failure(r,422,'invalid_input');c.no_queries(r)
@case('REL-001')
def rel001(c):
    for amount in [0,1,100,105]:
        c.sql('TRUNCATE todo');ids=[]
        for i in range(amount):identifier=str(uuid.UUID(int=0x40008000000000000100+i));ids.append(identifier);c.todo(identifier)
        c.todo(T2,owner=BOB);c.todo(T1,deleted=NOW);r=c.request('GET',f'/users/{ALICE}/todos');out=c.success(r);require(set(out)=={'id','todos'} and out['id']==ALICE,'safe parent projection');require([x['id'] for x in out['todos']]==sorted(ids)[:100],'owner/lifecycle before stable child cap');require(r['counts']['business']==2,'exact two statements independent of child count')
@case('REL-002')
def rel002(c):
    result=c.call('sql',sql=f'''INSERT INTO todo(id,owner_id,title,status,due_at,reminder_sent_at,reminder_queued_at,reminder_revision,deleted_at,created_at,updated_at) VALUES ('{T1}','{MISSING}','Invalid','open',NULL,NULL,NULL,1,NULL,'{NOW}','{NOW}')''');require(result.get('error','').startswith('Conflict'),'missing owner normalized store conflict');require(c.scalar('SELECT count(*) FROM todo')==0,'no orphan persisted')
@case('REL-003')
def rel003(c):
    c.todo();before=c.row()
    for method,path,body in [('POST','/todos',{'title':'Injected','owner_id':BOB}),('PATCH','/todos/'+T1,{'owner_id':BOB})]:
        c.failure(c.request(method,path,body=body),422,'invalid_input')
        c.unchanged(before)
    result=c.call('policy_probe');require(result['releaseEquivalentSuccess'] is False and result['disposition']=='requires_human_decision','ordinary owner mutation cannot ship');require(result['rule']=='type.field_update_forbidden','owner policy static rejection')
@case('REL-004')
def rel004(c):
    c.todo();c.sql(f'''UPDATE "user" SET status='disabled' WHERE id='{ALICE}' ''');r=c.call('sql',sql=f'''DELETE FROM "user" WHERE id='{ALICE}' ''');require(r.get('error','').startswith('Conflict'),'retained relationship restricts parent removal');require(c.row() is not None,'retained todo remains')
@case('PATCH-001')
def patch001(c):
    c.todo();before=c.row();c.success(c.request('PATCH','/todos/'+T1,body={'title':'Changed'}));after=c.row();require(after['title']=='Changed' and after['due_at']==before['due_at'] and after['reminder_revision']==before['reminder_revision'],'omission preserves due revision')
@case('PATCH-002')
def patch002(c):
    c.todo();out=c.success(c.request('PATCH','/todos/'+T1,body={'due_at':None}));require(out['due_at'] is None and c.row()['due_at'] is None,'explicit null clears nullable persisted due date')
@case('PATCH-003')
def patch003(c):
    c.todo();before=c.row();c.success(c.request('PATCH','/todos/'+T1,body={}));c.unchanged(before)
@case('PATCH-004')
def patch004(c):
    c.todo(T2,owner=BOB);before=c.row(T2);c.failure(c.request('PATCH','/todos/'+T2,body={'title':'Stolen'}),404,'todo_not_found');c.unchanged(before,T2)
@case('PATCH-005')
def patch005(c):
    c.todo(sent='2026-01-10T00:00:00Z');c.success(c.request('PATCH','/todos/'+T1,body={'due_at':'2026-02-15T09:00:00Z'}));after=c.row();require(after['due_at'].startswith('2026-02-15T09:00:00') and after['reminder_sent_at'] is None and after['reminder_revision']==2,'new schedule clears receipt and advances revision')
@case('DELETE-001')
def delete001(c):
    c.todo();c.success(c.request('DELETE','/todos/'+T1),204);row=c.row();require(row and row['deleted_at'].startswith('2026-01-15T12:00:00'),'soft delete retained timestamp');c.failure(c.request('GET','/todos/'+T1),404,'todo_not_found')
@case('DELETE-002')
def delete002(c):
    c.todo(deleted=NOW);before=c.row();c.failure(c.request('DELETE','/todos/'+T1),404,'todo_not_found');c.unchanged(before)
@case('USER-001')
def user001(c):
    c.todo();c.success(c.request('DELETE','/users/'+ALICE),204);require(c.row() is not None,'todo retained');require(c.scalar(f'''SELECT status FROM "user" WHERE id='{ALICE}' ''')=='disabled','persisted disable')
    for at,method,path,status,code in [(MILLIS+1000,'DELETE','/users/'+ALICE,403,'user_disabled'),(MILLIS+2000,'GET','/todos',403,'user_disabled'),(MILLIS+240000,'GET','/todos',403,'user_disabled'),(MILLIS+30*86400000-60000,'GET','/todos',401,'invalid_credentials')]:
        if at==MILLIS+2000:
            # Corrupt only the signed part: the still-live durable session must recheck authority.
            original=c.auth['alice-session'][0][1];c.auth['alice-session'][0][1]=original.split('.')[0]+'.invalid.signature'
        c.call('clock',at=at);r=c.request(method,path);c.failure(r,status,code)
        if at==MILLIS+2000:c.auth['alice-session'][0][1]=original
        require(not any(k.lower() in ['set-cookie','x-jadpo-token'] and 'Max-Age=0' not in v for k,v in r['headers']),'no new credentials on refused refresh')
@case('JOB-001')
def job001(c):
    c.todo(due='2026-01-15T11:00:00Z');c.job();mail=c.mail();require(len(mail['calls'])==1 and mail['accepted']==1,'one acknowledged send');call=mail['calls'][0];require(call['body']['to']=='alice@example.test' and call['body']['todo_id']==T1,'current owner recipient');require(call['key'].startswith('jd1_'),'runtime scoped provider key');require(dt.datetime.fromisoformat(c.row()['reminder_sent_at'])==dt.datetime.fromisoformat(mail['receipts'][0]['accepted_at'].replace('Z','+00:00')),'persisted sent time equals provider acknowledgement')
@case('JOB-002')
def job002(c):
    c.todo(due='2026-01-15T11:00:00Z');c.call('mail',answers=['lost','accepted']);c.job();mail=c.mail();require(mail['accepted']==1 and len(mail['calls'])==2,'lost acknowledgement safely replays');require(mail['calls'][0]['key']==mail['calls'][1]['key'] and mail['calls'][0]['body']==mail['calls'][1]['body'],'same key and immutable payload');c.call('clock',at=MILLIS+60000);c.call('tick');require(len(c.mail()['calls'])==2,'completed reminder not resent');require(c.row()['reminder_sent_at'] is not None,'successful replay stored receipt')
    # ABA and title/email changes after preparation are named variants, not silently omitted.
    reminder_races(c)
@case('JOB-003')
def job003(c):
    c.todo(status='done',due='2026-01-15T11:00:00Z');c.todo(T2,due='2026-01-15T11:00:00Z',deleted=NOW);c.todo(MISSING,due=None);c.todo(str(uuid.uuid4()),due='2026-01-15T11:00:00Z',sent='2026-01-15T11:30:00Z');c.todo(str(uuid.uuid4()),owner=BOB,due='2026-01-15T11:00:00Z');c.sql(f'''UPDATE "user" SET status='disabled' WHERE id='{BOB}' ''');c.job();require(c.mail()['calls']==[],'all excluded states skipped');disabled_after_preparation(c)
@case('JOB-004')
def job004(c):
    c.todo(deleted='2025-12-17T12:00:00Z');c.todo(T2,deleted='2025-12-15T12:00:00Z');c.todo(MISSING);c.call('clock',at=MILLIS-3600000);c.call('tick');c.call('clock',at=MILLIS);c.call('tick');require(c.row(T2) is None and c.row() and c.row(MISSING),'configured retention only old hidden row');purge_boundaries(c)
def preflight_failure(result):
    require(result['refused'] and not result['handlerRan'],'configuration refuses application')
    require(result['safe'],'preflight never echoes secrets')
    if 'status' in result:
        require(result['status']==503,'Worker startup refusal is503')
        error=result['body']['error'];require(error['code']=='unavailable' and bool(error['request_id']),'safe Worker startup error')
        require(dict((k.lower(),v) for k,v in result['headers']).get('x-request-id')==error['request_id'],'Worker startup nested/header request id match')

@case('CONFIG-001')
def config001(c):
    result=c.call('preflight',environment={'MAIL_API_KEY':None});preflight_failure(result);require(result['refused'] and not result['handlerRan'],'missing secret refuses application');require('MAIL_API_KEY' in result['declarations'] and 'setting_missing' in result['diagnosticCodes'],'secret declaration diagnostic');require(result['safe'],'preflight never echoes secrets')
@case('CONFIG-002')
def config002(c):
    result=c.call('preflight',environment={'OIDC_ISSUER':'not a url','DATABASE_URL':'postgres://user:password@'});preflight_failure(result);require(result['refused'] and not result['handlerRan'],'malformed config refuses application');require(set(result['declarations']) >= {'OIDC_ISSUER','DATABASE_URL'},f'all bad declarations diagnosed: {result}');require(result['safe'],'credential text never disclosed')
@case('CONFIG-003')
def config003(c):
    initial=c.success(c.request('GET','/health/ready',auth=None));require(initial['integrations']=={'reminder_mail':'unchecked'},'declared integration is named and never probed');c.call('clock',at=MILLIS+2000);c.call('outage',database=True);r=c.request('GET','/health/ready',auth=None);require(r['status']==503 and r['json']['status']=='not_ready' and r['json']['stores']['main']=='unavailable' and r['json']['integrations']=={'reminder_mail':'unchecked'},'bounded safe deployment readiness failure');require(c.success(c.request('GET','/health/live',auth=None))=={'status':'live'},'outage preserves liveness');c.call('outage',database=False);c.call('clock',at=MILLIS+4000);c.success(c.request('GET','/health/ready',auth=None));readiness_plane(c)
@case('BOUNDARY-001')
def boundary001(c):
    c.call('outage',database=True);r=c.request('GET','/todos');c.failure(r,503,'unavailable');c.call('outage',database=False);c.todo(due='2026-01-15T11:00:00Z');c.call('mail',answers=['unauthorized']);c.job();require(c.row()['reminder_sent_at'] is None,'failed provider never recorded as sent');safe_job_fault(c)

def validate_registry(contract):
    expected={case['id'] for case in contract['cases']}
    require(expected==set(CASES),f'case registry differs: missing={expected-set(CASES)}, extra={set(CASES)-expected}')
    require(len(expected)==44,'44 unique cases required')
    require(Counter(c['kind'] for c in contract['cases'])==Counter(http=34,job=4,persistence=2,preflight=2,policy=1,runtime=1),'all case kinds retained')


def reminder_races(c):
    # ABA before a due time still creates distinct durable schedule revisions.
    c.reset();c.todo(due='2026-02-01T00:00:00Z')
    for due in ['2026-02-02T00:00:00Z','2026-02-01T00:00:00Z']:
        c.success(c.request('PATCH','/todos/'+T1,body={'due_at':due}))
    require(c.row()['reminder_revision']==3,'ABA increments rather than comparing time/due equality')
    # A stale queued revision cannot mark or block a newly scheduled revision.
    c.reset();c.todo(due='2026-01-15T11:00:00Z');c.call('schedule',name='overdue_reminders')
    c.success(c.request('PATCH','/todos/'+T1,body={'due_at':'2026-01-16T09:00:00Z'}))
    c.call('drain');require(c.mail()['calls']==[] and c.row()['reminder_queued_at'] is None,'stale queue suppressed and new revision unblocked')
    c.call('clock',at=MILLIS+86400000);c.call('schedule',name='overdue_reminders');c.call('drain')
    require(c.mail()['accepted']==1 and str(c.mail()['calls'][0]['body']['revision'])=='2','new due revision eventually sends')
    # Prepare an immutable request, then edit title and provisioning email.
    c.reset();c.todo(due='2026-01-15T11:00:00Z');c.call('schedule',name='overdue_reminders');c.call('deliver_one')
    c.sql(f"UPDATE todo SET title='Edited after preparation' WHERE id='{T1}'")
    c.sql(f"UPDATE \"user\" SET email='new@example.test' WHERE id='{ALICE}'".replace('\\"','"'))
    c.call('mail',answers=['lost','accepted']);c.call('drain');mail=c.mail()
    require(mail['accepted']==1 and len(mail['calls'])==2,'prepared lost acknowledgement deduplicated')
    require(all(call['body']['title']=='Original' and call['body']['to']=='alice@example.test' for call in mail['calls']),'prepared payload immutable after title/email edit')
    # A prepared stale due revision is rejected too.
    c.reset();c.todo(due='2026-01-15T11:00:00Z');c.call('schedule',name='overdue_reminders');c.call('deliver_one')
    c.success(c.request('PATCH','/todos/'+T1,body={'due_at':'2026-01-16T09:00:00Z'}));c.call('drain')
    require(c.mail()['calls']==[] and c.row()['reminder_queued_at'] is None,'prepared stale revision does not send/block')
    # An edit while the provider is out must trigger the ordinary row-version guard.
    c.reset();c.todo(due='2026-01-15T11:00:00Z')
    c.call('mail',mutate=f"UPDATE todo SET due_at='2026-01-16T09:00:00Z',reminder_revision=reminder_revision+1,reminder_queued_at=NULL,reminder_sent_at=NULL WHERE id='{T1}'")
    c.job();require(c.mail()['accepted']==1,'already dispatched old mail cannot be retracted')
    row=c.row();require(row['reminder_revision']==2 and row['reminder_sent_at'] is None and row['reminder_queued_at'] is None,'old acknowledgement cannot mark edited schedule sent')
    old_key=c.mail()['calls'][0]['key'];c.call('clock',at=MILLIS+86400000);c.call('tick');mail=c.mail()
    require(mail['accepted']==2 and mail['calls'][-1]['key']!=old_key,'new revision has distinct delivery key')
    # The same HTTP503 fixture on both targets accepts no message and remains safely keyed.
    c.reset();c.todo(due='2026-01-15T11:00:00Z');c.call('mail',answers=['unavailable']*3);c.job()
    require(c.mail()['accepted']==0 and c.row()['reminder_sent_at'] is None,'provider outage is not a receipt')
    c.call('clock',at=MILLIS+60000);c.call('tick');require(c.mail()['accepted']==1 and c.row()['reminder_sent_at'] is not None,'safe provider outage retries')
    # Concurrent tick leases must execute the schedule only once.
    c.reset();c.todo(due='2026-01-15T11:00:00Z');c.call('clock',at=MILLIS-60000);c.call('tick');c.call('clock',at=MILLIS);c.call('concurrent_tick')
    require(c.mail()['accepted']==1 and len(c.mail()['calls'])==1,'concurrent scheduler claims send once')
    # A queue read changed before its write must rerun, not mark the new revision.
    c.reset();c.todo(due='2026-01-15T11:00:00Z')
    c.call('mutation_hook',match='reminder_queued_at',sql=f"UPDATE todo SET due_at='2026-01-16T09:00:00Z',reminder_revision=2 WHERE id='{T1}'")
    c.call('schedule',name='overdue_reminders');c.call('drain')
    require(c.mail()['accepted']==0 and c.row()['reminder_revision']==2 and c.row()['reminder_queued_at'] is None,'concurrent queue edit keeps new schedule unqueued')
    # Once a keyed send may have happened, elapsed provider safety window (24h minus its 1h margin) parks it.
    c.reset();c.todo(due='2026-01-15T11:00:00Z');c.call('mail',answers=['lost']*3);c.job()
    require(c.mail()['accepted']==1 and c.row()['reminder_sent_at'] is None,'unknown acknowledgement is not recorded as successful')
    attempts=len(c.mail()['calls']);c.call('clock',at=MILLIS+23*3600000+1);c.call('sweep')
    require(len(c.mail()['calls'])==attempts,'unknown outcome after key window never sent blindly')
    states=[row[0] for row in c.sql("SELECT status FROM jadpo_deliveries WHERE subscriber LIKE '%send_reminder%'")]
    require(states==['outcome_unknown'],f'unknown outcome parked visibly: {states}')

def disabled_after_preparation(c):
    c.reset();c.todo(due='2026-01-15T11:00:00Z');c.call('schedule',name='overdue_reminders');c.call('deliver_one')
    c.sql(f"UPDATE \"user\" SET status='disabled' WHERE id='{ALICE}'".replace('\\"','"'));c.call('drain')
    require(c.mail()['calls']==[] and c.row()['reminder_sent_at'] is None,'owner disabled before dispatch suppresses prepared mail')

def purge_boundaries(c):
    c.reset();c.todo(deleted='2025-12-16T12:00:00Z');c.todo(T2,deleted='2025-12-16T11:59:59Z')
    c.call('clock',at=MILLIS-3600000);c.call('tick');c.call('clock',at=MILLIS);c.call('tick')
    require(c.row() is not None and c.row(T2) is None,'retention boundary uses older-than configured duration')
    c.reset()
    c.sql(f"INSERT INTO todo(id,owner_id,title,status,due_at,reminder_sent_at,reminder_queued_at,reminder_revision,deleted_at,created_at,updated_at) SELECT ('10000000-0000-4000-8000-'||lpad(to_hex(i),12,'0'))::uuid,'{ALICE}','Old','open',NULL,NULL,NULL,1,'2025-12-01T00:00:00Z','2025-11-01T00:00:00Z','2025-12-01T00:00:00Z' FROM generate_series(1,10001) i")
    c.call('clock',at=MILLIS-3600000);c.call('tick');c.call('clock',at=MILLIS);c.call('tick')
    statements=c.history[-1]['statements'];deletes=[sql for sql in statements if sql.startswith('DELETE FROM "todo"')]
    require(len(deletes)==20 and all('LIMIT 500' in sql for sql in deletes),'generated run does twenty bounded 500-row batches')
    require(c.scalar('SELECT count(*) FROM todo')==1,'twenty-batch cap leaves remainder for next run')
    require(c.scalar("SELECT items FROM jadpo_schedule_runs WHERE schedule LIKE '%Todo.purge%' AND outcome='succeeded' ORDER BY started_at DESC LIMIT 1")==10000,'generated run reports actual purged items')
    c.call('clock',at=MILLIS+3600000);c.call('tick');require(c.scalar('SELECT count(*) FROM todo')==0,'next run purges remainder')
    c.call('clock',at=MILLIS+7200000);c.call('tick');require(c.scalar('SELECT count(*) FROM todo')==0,'purge replay idempotent')

def readiness_plane(c):
    # The generated API record must not expose either runtime probe.
    result=c.call('api_record')
    require('/health/ready' not in result['record'] and '/health/live' not in result['record'],'runtime probes are outside application API')

def safe_job_fault(c):
    rows=c.sql("SELECT id::text,last_failure::text FROM jadpo_deliveries WHERE subscriber LIKE '%send_reminder%'")
    require(len(rows)==1 and rows[0][0] and rows[0][1],'background failure has durable correlation identity')
    for forbidden in ['private-provider-secret','SELECT ','password@','stack']:
        require(forbidden not in rows[0][1],'background failure does not disclose internals')
