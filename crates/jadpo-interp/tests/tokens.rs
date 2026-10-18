//! Area 19, "Hashing and cryptography" (chunk 5.5r), run:
//! `crypto.verify_jwt` gives a genuine token's claims and refuses `none`, an
//! algorithm the token chooses, a tampered payload, an expired token, one
//! not yet valid, and one for another audience or from another issuer,
//! each as the failure its arm names, against the invocation's clock.

use std::rc::Rc;

use crate::support;

use jadpo_interp::{Executor, Outcome, Program, Value};
use jadpo_runtime::{Clock, Ctx, Env, MemoryHost};
use support::main;

const SOURCE: &str = "type BadSignature = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TokenExpired = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TokenEarly = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype NotAToken = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype OtherAudience = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype OtherIssuer = Failure {\n    kind: FailureKind.invalid_value\n}\n\naction verified(token: Jwt, keys: List<Jwk>) fails BadSignature, TokenExpired, TokenEarly, NotAToken, OtherAudience, OtherIssuer -> JwtClaims {\n    return attempt crypto.verify_jwt(token, keys: keys, issuer: \"https://id.example.com\", audience: \"clinic\") {\n        invalid_signature: BadSignature\n        expired: TokenExpired\n        not_yet_valid: TokenEarly\n        malformed: NotAToken\n        wrong_audience: OtherAudience\n        wrong_issuer: OtherIssuer\n    }\n}\n";

/// 2026-10-18T09:00:00Z, the clock the tokens' times are around.
const NOW: i64 = 1_792_314_000_000;

// Signed with RFC 8032's first test key, by its id `2026-10`, unless the
// name says otherwise.
const GOOD: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.nOqN7320O3T8e_AyF7XNt32jmgjE8VCU7GVHL89ZUeiRcU3gGM3EgiNxVp3_y422De1PZWd2tZ0LuSqdfm6QCg";
const EXPIRED: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzEzOTY5LCJqdGkiOiJhMWIyYzMifQ.0XDMfIlEkopB8bJAJCe95i2zQMGu9mbv_kUtc1QC0xMm5wxmsS9munCCG36_4NkaNcfDKNgaGpswq5QNuh_KDw";
const NOT_YET: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzIxMjAwLCJuYmYiOjE3OTIzMTc2MDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.NEOn2CiCjIrOo3-lfR85g4KGRcmsDvmDFxLoSa_pCxov4sxnnaq4mVgiwKpIEglhlUzNAN5f9x1_8_4pv9JuBA";
const OTHER_AUDIENCE: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoicGhhcm1hY3kiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.ysn61bX4ZDaJ7OCiuPNtYwJ1POMD7b1tVLIfZsDS1oimtwY3XgyhZDpLSkDph6j3fol8nv4KPZQ0g7Ai9pd4Bw";
const OTHER_ISSUER: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2V2aWwuZXhhbXBsZS5jb20iLCJzdWIiOiJtZW1iZXItNDIiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.Gmk9tIA-Ivmifdrhf0jKtGXPD87trYF59gq4DB0b91vwD1zAKRzpLhnsxOFYmK9UF8py09jUVBq81sKlhcGZAA";
const ALG_NONE: &str = "eyJhbGciOiJub25lIiwia2lkIjoiMjAyNi0xMCJ9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.bUBJ-HYfcv-x-XVebqotpXUryxrUsu15W2sejqXFebmY2OB3-XXAQIWG71XLjOBQDrJnDyfN4RTsPW20OqqBDA";
const HS256_PUBLIC_TEXT: &str = "eyJhbGciOiJIUzI1NiIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.LEqGD4Thr8CB21DsacQjPDe-b_CRUkja0n3gjc6sT7M";
const TAMPERED: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTEiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.nOqN7320O3T8e_AyF7XNt32jmgjE8VCU7GVHL89ZUeiRcU3gGM3EgiNxVp3_y422De1PZWd2tZ0LuSqdfm6QCg";
const NO_EXPIRY: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwibmJmIjoxNzkyMzEzOTQwLCJpYXQiOjE3OTIzMTM5NDAsImp0aSI6ImExYjJjMyJ9.U70xc8AeLGIBUiBSymvHQ4KTyfwxGFF2KRVIUwJ7NYmWD1vJq7RSJxksR3_Qgy-c3ylShb1NPYtDj0eiqiPjDw";

/// RFC 8037 §A.2's public key, as a key set sends it.
fn keys() -> Value {
    Value::list(vec![Value::record(
        "Jwk",
        vec![
            (Rc::from("kty"), Value::text("OKP")),
            (Rc::from("crv"), Value::text("Ed25519")),
            (
                Rc::from("x"),
                Value::text("11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo"),
            ),
            (Rc::from("kid"), Value::text("2026-10")),
            (Rc::from("alg"), Value::None),
            (Rc::from("use_"), Value::None),
        ],
    )])
}

/// `verified(token, keys())` at `now`.
fn verified(program: &Program, token: &str, now: i64) -> Outcome {
    let id = program.plan().find("verified").expect("verified");
    let host = MemoryHost::new();
    let mut ctx = Ctx::new(
        program.places(),
        &host,
        Env {
            seed: Some(7),
            clock: Clock::fixed(now),
        },
    );
    jadpo_runtime::block_on(program.run(&mut ctx, id, vec![Value::text(token), keys()]))
        .expect("ran")
}

fn refused(program: &Program, token: &str) -> String {
    match verified(program, token, NOW) {
        Outcome::Failed(failure) => failure.name.clone(),
        other => panic!("{token}: {other:?}"),
    }
}

#[test]
fn a_genuine_token_gives_its_claims_at_the_invocation_s_time() {
    let program = main(SOURCE);
    let Outcome::Value(Value::Record(claims)) = verified(&program, GOOD, NOW) else {
        panic!("a genuine token is refused");
    };
    assert_eq!(claims.get("sub"), Some(&Value::text("member-42")));
    assert_eq!(
        claims.get("aud"),
        Some(&Value::list(vec![Value::text("clinic")]))
    );
    assert_eq!(claims.get("exp"), Some(&Value::Instant(NOW + 3_600_000)));
    // The same token, two hours on, has expired; the clock is the
    // invocation's, never the real one.
    match verified(&program, GOOD, NOW + 7_200_000) {
        Outcome::Failed(failure) => assert_eq!(failure.name, "TokenExpired"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_way_a_token_is_refused_is_its_own_arm() {
    let program = main(SOURCE);
    for (token, failure) in [
        (ALG_NONE, "BadSignature"),
        (HS256_PUBLIC_TEXT, "BadSignature"),
        (TAMPERED, "BadSignature"),
        (EXPIRED, "TokenExpired"),
        (NOT_YET, "TokenEarly"),
        (NO_EXPIRY, "NotAToken"),
        (OTHER_AUDIENCE, "OtherAudience"),
        (OTHER_ISSUER, "OtherIssuer"),
    ] {
        assert_eq!(refused(&program, token), failure, "{token}");
    }
}
