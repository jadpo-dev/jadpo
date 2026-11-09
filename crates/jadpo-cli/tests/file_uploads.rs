//! Uploads against live PostgreSQL (area 16, "Uploading"; chunk B5): a
//! bakery's suppliers send their price sheets straight to the store. A
//! route asks for a slot, the client sends the file to the slot's address,
//! and an ordinary create or update attaches it. Every refusal is this
//! project's route tests, run interpreted here and compiled, compared with
//! the interpreter's records, in the gate's compiled step (`compiled.rs`,
//! "a file uploaded").
//!
//! A large file goes in parts ("Large uploads go in parts"), each resent
//! freely, and an ordinary attach joins them: the project's route tests do,
//! and so, over real HTTP with a connection truly dropped, does
//! [`serves_an_upload_in_parts`].
//!
//! What only a real client can see, the file landing on a real disk and a
//! body cut off part-way, is [`serves_an_upload`]: over real HTTP from
//! `jadpo dev` (`disk_files.rs`) and a compiled deployable (`deployable.rs`).
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const OPEN: &str = "    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
";

/// A slot's action, a store's file's entity with a field `field`, and its
/// routes.
fn with_file(
    plain: &mut Vec<(&'static str, String)>,
    (entity_file, entity, field, settings): (&'static str, &str, &str, &str),
    (action_file, action_name): (&'static str, &str),
) {
    plain.push((
        entity_file,
        format!("/// {entity}.\nentity {entity} {{\n    id: Uuid\n    {field}: File {{\n        store: sheets\n        {settings}\n    }}\n\n{OPEN}}}\n"),
    ));
    plain.push((
        action_file,
        format!("use bakery/entities {{ {entity} }}\n\n/// A slot for a {entity}'s {field}.\naction {action_name}() -> UploadSlot {{\n    return {entity}.{field}_upload()\n}}\n"),
    ));
}

/// A bakery's price sheets, each kept by an upload, and what else its
/// tests attach.
pub(crate) fn files(path: &str) -> Vec<(String, String)> {
    let app = format!(
        "use bakery/entities {{ Clerk }}\n\napplication {{\n    connections: {{\n        main: {{\n            driver: Driver.postgres\n            url: config.database_url\n        }}\n        sheets: {{\n            driver: Driver.disk\n            path: \"{path}\"\n        }}\n    }}\n}}\n\nprincipal {{\n    user: {{\n        subject: Clerk.handle\n    }}\n}}\n\nauthentication {{\n    api_token: {{}}\n}}\n"
    );
    let mut plain: Vec<(&str, String)> = vec![
        ("app.jadpo", app),
        (
            "config.jadpo",
            "config {\n    /// The bakery's database.\n    database_url: PostgresUrl\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/supplier_name.jadpo",
            "/// Who sends price sheets.\ntype SupplierName = Text {\n    min_length: 1\n    max_length: 80\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/new_sheet.jadpo",
            "use bakery/types { SupplierName }\n\n/// A sheet to keep, once it is uploaded.\ntype NewSheet = Input {\n    supplier: SupplierName\n    sheet: FileUpload\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/new_notes.jadpo",
            "/// A sheet's notes, once they are uploaded.\ntype NewNotes = Input {\n    notes: FileUpload\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/new_upload.jadpo",
            "/// A file to keep, once it is uploaded.\ntype NewUpload = Input {\n    upload: FileUpload\n}\n".to_string(),
        ),
        (
            "domains/bakery/failures/handle_taken.jadpo",
            "/// Someone has that handle already.\ntype HandleTaken = Failure {\n    kind: FailureKind.conflict\n}\n".to_string(),
        ),
        (
            "domains/bakery/entities/price_sheet.jadpo",
            format!("use bakery/types {{ SupplierName }}\n\n/// A supplier's price sheet, kept as it was sent.\nentity PriceSheet {{\n    id: Uuid\n    supplier: SupplierName\n    sheet: File {{\n        store: sheets\n        max_size: 64b\n        types: [FileType.csv]\n    }}\n    notes: File? {{\n        store: sheets\n        max_size: 1kb\n        types: [FileType.pdf]\n    }}\n\n{OPEN}}}\n"),
        ),
        (
            "domains/bakery/entities/clerk.jadpo",
            format!("/// A shop assistant who signs in.\nentity Clerk {{\n    id: Uuid\n    handle: Text {{\n        max_length: 80\n        unique: true\n    }}\n\n{OPEN}}}\n"),
        ),
        (
            "domains/bakery/types/staff_role.jadpo",
            "/// What a clerk may do across the bakery.\ntype StaffRole = Enum {\n    admin\n    clerk\n}\n".to_string(),
        ),
        (
            "domains/bakery/entities/staff_member.jadpo",
            format!("use bakery/entities {{ Clerk }}\nuse bakery/types {{ StaffRole }}\n\n/// A clerk's role across the whole bakery.\nentity StaffMember {{\n    id: Uuid\n    clerk_id: Clerk.id {{\n        on_delete: OnDelete.cascade\n    }}\n    role: StaffRole\n\n    membership: {{\n        scope: MembershipScope.application\n        member: clerk_id\n        role: role\n    }}\n\n{OPEN}}}\n"),
        ),
        (
            "domains/bakery/entities/ledger.jadpo",
            "use bakery/types { StaffRole }\n\n/// The bakery's ledger, which only an admin may keep.\nentity Ledger {\n    id: Uuid\n    page: File {\n        store: sheets\n        max_size: 1kb\n    }\n\n    policy: {\n        create: [StaffRole.admin]\n        read: [StaffRole.admin]\n    }\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/sheet_view.jadpo",
            "use bakery/entities { PriceSheet }\n\n/// A sheet and the link to its file.\ntype SheetView = Output {\n    pick: [PriceSheet.id, PriceSheet.supplier, PriceSheet.sheet, PriceSheet.notes]\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/scan_view.jadpo",
            "use bakery/entities { Scan }\n\n/// A scan kept.\ntype ScanView = Output {\n    pick: [Scan.id]\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/kept_view.jadpo",
            "use bakery/entities { Attachment }\n\n/// A file kept and the link to it.\ntype KeptView = Output {\n    pick: [Attachment.id, Attachment.file]\n}\n".to_string(),
        ),
        (
            "domains/bakery/actions/join.jadpo",
            "use bakery/entities { Clerk }\nuse bakery/failures { HandleTaken }\n\n/// A clerk joins the bakery.\naction join(handle: Clerk.handle) fails HandleTaken -> Clerk {\n    return attempt Clerk.create {\n        set: {\n            handle: handle\n        }\n        conflict: {\n            Clerk.handle: HandleTaken\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/bakery/actions/make_admin.jadpo",
            "use bakery/entities { Clerk, ClerkNotFound, StaffMember }\nuse bakery/types { StaffRole }\n\n/// Makes a clerk an admin.\naction make_admin(clerk_id: Clerk.id) fails ClerkNotFound -> StaffMember {\n    return attempt StaffMember.create {\n        set: {\n            clerk_id: clerk_id\n            role: StaffRole.admin\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/bakery/actions/keep_sheet.jadpo",
            "use bakery/entities { PriceSheet }\nuse bakery/types { NewSheet, SheetView }\n\n/// Attaches an uploaded sheet to a new record.\naction keep_sheet(input: NewSheet) -> SheetView {\n    var kept: PriceSheet = PriceSheet.create {\n        set: {\n            supplier: input.supplier\n            sheet: input.sheet\n            notes: none\n        }\n    }\n    return SheetView {\n        id: kept.id\n        supplier: kept.supplier\n        sheet: kept.sheet\n        notes: kept.notes\n    }\n}\n".to_string(),
        ),
        (
            "domains/bakery/actions/add_notes.jadpo",
            "use bakery/entities { PriceSheet, PriceSheetNotFound }\nuse bakery/types { NewNotes, SheetView }\n\n/// Attaches uploaded notes to a record read first.\naction add_notes(sheet_id: PriceSheet.id, input: NewNotes) fails PriceSheetNotFound -> SheetView {\n    var sheet: PriceSheet = attempt PriceSheet.get(sheet_id)\n    var kept: PriceSheet = PriceSheet.update(sheet) {\n        set: {\n            notes: input.notes\n        }\n    }\n    return SheetView {\n        id: kept.id\n        supplier: kept.supplier\n        sheet: kept.sheet\n        notes: kept.notes\n    }\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/archive_view.jadpo",
            "use bakery/entities { Archive }\n\n/// An archive kept and the link to it.\ntype ArchiveView = Output {\n    pick: [Archive.id, Archive.file]\n}\n".to_string(),
        ),
        (
            "domains/bakery/actions/keep_archive.jadpo",
            "use bakery/entities { Archive }\nuse bakery/types { ArchiveView, NewUpload }\n\n/// Attaches an archive uploaded in parts.\naction keep_archive(input: NewUpload) -> ArchiveView {\n    var kept: Archive = Archive.create {\n        set: {\n            file: input.upload\n        }\n    }\n    return ArchiveView {\n        id: kept.id\n        file: kept.file\n    }\n}\n".to_string(),
        ),
        (
            "domains/bakery/actions/keep_scan.jadpo",
            "use bakery/entities { Scan }\nuse bakery/types { NewUpload, ScanView }\n\n/// Attaches an uploaded scan.\naction keep_scan(input: NewUpload) -> ScanView {\n    var kept: Scan = Scan.create {\n        set: {\n            document: input.upload\n        }\n    }\n    return ScanView {\n        id: kept.id\n    }\n}\n".to_string(),
        ),
        (
            "domains/bakery/actions/keep_attachment.jadpo",
            "use bakery/entities { Attachment }\nuse bakery/types { KeptView, NewUpload }\n\n/// Attaches an uploaded file of any type.\naction keep_attachment(input: NewUpload) -> KeptView {\n    var kept: Attachment = Attachment.create {\n        set: {\n            file: input.upload\n        }\n    }\n    return KeptView {\n        id: kept.id\n        file: kept.file\n    }\n}\n".to_string(),
        ),
        (
            "domains/bakery/actions/sheet_slot.jadpo",
            "use bakery/entities { PriceSheet }\n\n/// A slot for a sheet.\naction sheet_slot() -> UploadSlot {\n    return PriceSheet.sheet_upload()\n}\n".to_string(),
        ),
        (
            "domains/bakery/actions/notes_slot.jadpo",
            "use bakery/entities { PriceSheet }\n\n/// A slot for a sheet's notes.\naction notes_slot() -> UploadSlot {\n    return PriceSheet.notes_upload()\n}\n".to_string(),
        ),
        (
            "domains/bakery/actions/ledger_slot.jadpo",
            "use bakery/entities { Ledger }\n\n/// A slot for a ledger's page.\naction ledger_slot() -> UploadSlot {\n    return Ledger.page_upload()\n}\n".to_string(),
        ),
        (
            "domains/bakery/routes/sheets.jadpo",
            r#"use bakery/actions { add_notes, archive_slot, attachment_slot, keep_archive, keep_attachment, keep_scan, keep_sheet, ledger_slot, notes_slot, scan_slot, sheet_slot }
use bakery/entities { PriceSheet }
use bakery/types { KeptView, NewNotes, NewSheet, NewUpload, ScanView, SheetView, ArchiveView }

routes Sheets {
    prefix: /sheets

    POST /slot {
        authentication: RouteAuthentication.none
        output: UploadSlot
        run: sheet_slot()
    }

    POST /notes-slot {
        authentication: RouteAuthentication.none
        output: UploadSlot
        run: notes_slot()
    }

    POST /ledger-slot {
        output: UploadSlot
        run: ledger_slot()
    }

    POST /scan-slot {
        authentication: RouteAuthentication.none
        output: UploadSlot
        run: scan_slot()
    }

    POST /archive-slot {
        authentication: RouteAuthentication.none
        output: UploadSlot
        run: archive_slot()
    }

    POST /attachment-slot {
        authentication: RouteAuthentication.none
        output: UploadSlot
        run: attachment_slot()
    }

    POST / {
        authentication: RouteAuthentication.none
        input: NewSheet
        output: SheetView
        status: SuccessStatus.created
        run: keep_sheet(input)
    }

    POST /{sheet_id: PriceSheet.id}/notes {
        authentication: RouteAuthentication.none
        input: NewNotes
        output: SheetView
        run: add_notes(sheet_id, input)
    }

    POST /scans {
        authentication: RouteAuthentication.none
        input: NewUpload
        output: ScanView
        status: SuccessStatus.created
        run: keep_scan(input)
    }

    POST /archives {
        authentication: RouteAuthentication.none
        input: NewUpload
        output: ArchiveView
        status: SuccessStatus.created
        run: keep_archive(input)
    }

    POST /attachments {
        authentication: RouteAuthentication.none
        input: NewUpload
        output: KeptView
        status: SuccessStatus.created
        run: keep_attachment(input)
    }
}
"#.to_string(),
        ),
        (
            "domains/bakery/routes/sheets.test.jadpo",
            r#"use bakery/actions { join, keep_sheet, make_admin }
use bakery/entities { Clerk, StaffMember }
use bakery/types { NewNotes, NewSheet, NewUpload, SheetView }

test "a sheet is uploaded, attached and downloaded" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/slot"
    }
    assert asked.status == 200
    var sent: Response = request {
        method: HttpMethod.put
        upload: asked.output
        body: encoding.to_utf8("flour,1.20\n")
        content_type: "text/csv"
        file_name: "prices.csv"
    }
    assert sent.status == 204
    assert sent.code == none
    var token: Text = match asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets"
        input: NewSheet {
            supplier: "Millers"
            sheet: token
        }
    }
    assert kept.status == 201
    var sheet: File? = match kept.output {
        some(view) -> view.sheet
        none -> none
    }
    var back: Response = request {
        method: HttpMethod.get
        link: sheet
    }
    assert back.status == 200
    assert back.body == encoding.to_utf8("flour,1.20\n")
}

test "an update attaches an upload to a record read first" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/slot"
    }
    var sent: Response = request {
        method: HttpMethod.put
        upload: asked.output
        body: encoding.to_utf8("flour,1.20\n")
    }
    assert sent.status == 204
    var token: Text = match asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var kept: SheetView = keep_sheet(NewSheet {
        supplier: "Millers"
        sheet: token
    })
    var notes_asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/notes-slot"
    }
    var notes_sent: Response = request {
        method: HttpMethod.put
        upload: notes_asked.output
        body: encoding.to_utf8("%PDF-1.7 notes")
    }
    assert notes_sent.status == 204
    var notes_token: Text = match notes_asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var updated: Response = request {
        method: HttpMethod.post
        path: "/sheets/{sheet_id}/notes"
        params: {
            sheet_id: kept.id
        }
        input: NewNotes {
            notes: notes_token
        }
    }
    assert updated.status == 200
    var notes: File? = match updated.output {
        some(view) -> view.notes
        none -> none
    }
    var back: Response = request {
        method: HttpMethod.get
        link: notes
    }
    assert back.status == 200
    assert back.body == encoding.to_utf8("%PDF-1.7 notes")
}

test "a slot's token is good after its address has expired, once the file is there" {
    clock: "2026-01-15T12:00:00Z"

    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/slot"
    }
    var sent: Response = request {
        method: HttpMethod.put
        upload: asked.output
        body: encoding.to_utf8("flour,1.20\n")
    }
    assert sent.status == 204
    clock.advance(16m)
    var token: Text = match asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets"
        input: NewSheet {
            supplier: "Millers"
            sheet: token
        }
    }
    assert kept.status == 201
}

test "an address expires after fifteen minutes" {
    clock: "2026-01-15T12:00:00Z"

    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/slot"
    }
    clock.advance(16m)
    var sent: Response = request {
        method: HttpMethod.put
        upload: asked.output
        body: encoding.to_utf8("flour,1.20\n")
    }
    assert sent.status == 403
    assert sent.code == "link_expired"
}

test "an altered address is refused" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/slot"
    }
    assert asked.status == 200
    var sent: Response = request {
        method: HttpMethod.put
        upload: "/files/sheets/uploads/upl_eyJrIjoiYSJ9.AAAA"
        body: encoding.to_utf8("flour,1.20\n")
    }
    assert sent.status == 403
    assert sent.code == "link_invalid"
}

test "a body over a slot's limit is refused at its first byte over" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/slot"
    }
    var sent: Response = request {
        method: HttpMethod.put
        upload: asked.output
        body: encoding.to_utf8("flour,1.20\nrye,2.40\nspelt,3.10\nbarley,1.90\noats,1.10\nmillet,4.00\n")
    }
    assert sent.status == 413
    assert sent.code == "file_too_large"
    var token: Text = match asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets"
        input: NewSheet {
            supplier: "Millers"
            sheet: token
        }
    }
    assert kept.status == 422
    assert kept.code == "invalid_value"
}

test "a file that isn't a type the slot takes is refused" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/slot"
    }
    var sent: Response = request {
        method: HttpMethod.put
        upload: asked.output
        body: encoding.to_utf8("%PDF-1.7 not a price sheet")
        content_type: "text/csv"
    }
    assert sent.status == 415
    assert sent.code == "file_type_refused"
}

test "a Content-Encoding is refused" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/slot"
    }
    var sent: Response = request {
        method: HttpMethod.put
        upload: asked.output
        body: encoding.to_utf8("flour,1.20\n")
        content_encoding: "gzip"
    }
    assert sent.status == 415
    assert sent.code == "content_encoding_refused"
}

test "a second file sent to a slot is refused" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/slot"
    }
    var first: Response = request {
        method: HttpMethod.put
        upload: asked.output
        body: encoding.to_utf8("flour,1.20\n")
    }
    assert first.status == 204
    var second: Response = request {
        method: HttpMethod.put
        upload: asked.output
        body: encoding.to_utf8("rye,2.40\n")
    }
    assert second.status == 409
    assert second.code == "upload_already_sent"
}

test "attaching before the file has landed is invalid_value" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/slot"
    }
    var token: Text = match asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets"
        input: NewSheet {
            supplier: "Millers"
            sheet: token
        }
    }
    assert kept.status == 422
    assert kept.code == "invalid_value"
}

test "an altered token is invalid_value" {
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets"
        input: NewSheet {
            supplier: "Millers"
            sheet: "upl_eyJrIjoiYSJ9.AAAA"
        }
    }
    assert kept.status == 422
    assert kept.code == "invalid_value"
}

test "a token for another field is invalid_value" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/notes-slot"
    }
    var sent: Response = request {
        method: HttpMethod.put
        upload: asked.output
        body: encoding.to_utf8("%PDF-1.7 notes")
    }
    assert sent.status == 204
    var token: Text = match asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets"
        input: NewSheet {
            supplier: "Millers"
            sheet: token
        }
    }
    assert kept.status == 422
    assert kept.code == "invalid_value"
}

test "the bytes tell a file's type, whatever was claimed" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/scan-slot"
    }
    var sent: Response = request {
        method: HttpMethod.put
        upload: asked.output
        body: encoding.to_utf8("%PDF-1.7 a scan")
        content_type: "text/csv"
        file_name: "scan.pdf"
    }
    assert sent.status == 204
    var token: Text = match asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets/scans"
        input: NewUpload {
            upload: token
        }
    }
    assert kept.status == 201
}

test "text kept in a field of any type needs the type claimed" {
    var unsaid: Response = request {
        method: HttpMethod.post
        path: "/sheets/attachment-slot"
    }
    var refused: Response = request {
        method: HttpMethod.put
        upload: unsaid.output
        body: encoding.to_utf8("flour,1.20\n")
    }
    assert refused.status == 415
    assert refused.code == "file_type_refused"
    var said: Response = request {
        method: HttpMethod.post
        path: "/sheets/attachment-slot"
    }
    var sent: Response = request {
        method: HttpMethod.put
        upload: said.output
        body: encoding.to_utf8("flour,1.20\n")
        content_type: "text/csv"
    }
    assert sent.status == 204
    var token: Text = match said.output {
        some(slot) -> slot.token
        none -> ""
    }
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets/attachments"
        input: NewUpload {
            upload: token
        }
    }
    assert kept.status == 201
}

test "a slot needs the entity's create or update grant" {
    var ada: Clerk = attempt join("ada")
    var refused: Response = request {
        method: HttpMethod.post
        path: "/sheets/ledger-slot"
        as: Principal.user(ada)
    }
    assert refused.status == 403
    assert refused.code == "not_permitted"
    var bea: Clerk = attempt join("bea")
    var made: StaffMember = attempt make_admin(bea.id)
    assert made.clerk_id == bea.id
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/ledger-slot"
        as: Principal.user(bea)
    }
    assert asked.status == 200
}

test "a large file goes in parts, one resent after a dropped connection, and is attached" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/archive-slot"
    }
    assert asked.status == 200
    var part_size: Int? = match asked.output {
        some(slot) -> slot.part_size
        none -> none
    }
    assert part_size == 16777216
    var plain_url: Text? = match asked.output {
        some(slot) -> slot.url
        none -> none
    }
    assert plain_url == none
    var first_part: Bytes = encoding.to_utf8(text.pad_start("", 16777216, fill: "a"))
    var first: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 1
        body: first_part
        content_type: "text/csv"
        file_name: "archive.csv"
    }
    assert first.status == 204
    var cut: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 2
        body: encoding.to_utf8("lost,part\n")
        dropped: true
    }
    assert cut.status == 400
    assert cut.code == "invalid_request"
    var resent: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 2
        body: encoding.to_utf8("end,part\n")
    }
    assert resent.status == 204
    var token: Text = match asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets/archives"
        input: NewUpload {
            upload: token
        }
    }
    assert kept.status == 201
    var file: File? = match kept.output {
        some(view) -> view.file
        none -> none
    }
    var back: Response = request {
        method: HttpMethod.get
        link: file
    }
    assert back.status == 200
    assert back.body == bytes.concat(first_part, encoding.to_utf8("end,part\n"))
}

test "a part resent replaces the one before it" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/archive-slot"
    }
    var first: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 1
        body: encoding.to_utf8("old,part\n")
    }
    assert first.status == 204
    var second: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 1
        body: encoding.to_utf8("new,part\n")
    }
    assert second.status == 204
    var token: Text = match asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets/archives"
        input: NewUpload {
            upload: token
        }
    }
    assert kept.status == 201
    var file: File? = match kept.output {
        some(view) -> view.file
        none -> none
    }
    var back: Response = request {
        method: HttpMethod.get
        link: file
    }
    assert back.body == encoding.to_utf8("new,part\n")
}

test "an attach waits for every part" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/archive-slot"
    }
    var first_part: Bytes = encoding.to_utf8(text.pad_start("", 16777216, fill: "a"))
    var first: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 1
        body: first_part
    }
    assert first.status == 204
    var third: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 3
        body: encoding.to_utf8("end,part\n")
    }
    assert third.status == 204
    var token: Text = match asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var early: Response = request {
        method: HttpMethod.post
        path: "/sheets/archives"
        input: NewUpload {
            upload: token
        }
    }
    assert early.status == 422
    assert early.code == "invalid_value"
    var middle: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 2
        body: first_part
    }
    assert middle.status == 204
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets/archives"
        input: NewUpload {
            upload: token
        }
    }
    assert kept.status == 201
}

test "parts that aren't equal can't be attached" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/archive-slot"
    }
    var short: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 1
        body: encoding.to_utf8("short,part\n")
    }
    assert short.status == 204
    var second: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 2
        body: encoding.to_utf8("end,part\n")
    }
    assert second.status == 204
    var token: Text = match asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets/archives"
        input: NewUpload {
            upload: token
        }
    }
    assert kept.status == 422
    assert kept.code == "invalid_value"
}

test "a part over the part size or past the last is too large" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/archive-slot"
    }
    var over: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 1
        body: encoding.to_utf8(text.pad_start("", 16777217, fill: "a"))
    }
    assert over.status == 413
    assert over.code == "file_too_large"
    var past: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 13
        body: encoding.to_utf8("a,b\n")
    }
    assert past.status == 413
    assert past.code == "file_too_large"
}

test "a part sent after the file is joined is refused" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/archive-slot"
    }
    var first: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 1
        body: encoding.to_utf8("a,b\n")
    }
    assert first.status == 204
    var token: Text = match asked.output {
        some(slot) -> slot.token
        none -> ""
    }
    var kept: Response = request {
        method: HttpMethod.post
        path: "/sheets/archives"
        input: NewUpload {
            upload: token
        }
    }
    assert kept.status == 201
    var again: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 1
        body: encoding.to_utf8("b,c\n")
    }
    assert again.status == 409
    assert again.code == "upload_already_sent"
}

test "a first part isn't a type the slot takes" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/archive-slot"
    }
    var sent: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 1
        body: encoding.to_utf8("%PDF-1.7 not a price sheet")
        content_type: "text/csv"
    }
    assert sent.status == 415
    assert sent.code == "file_type_refused"
}

test "a part with a Content-Encoding is refused" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/archive-slot"
    }
    var sent: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 1
        body: encoding.to_utf8("a,b\n")
        content_encoding: "gzip"
    }
    assert sent.status == 415
    assert sent.code == "content_encoding_refused"
}

test "a slot in parts is good for six hours" {
    clock: "2026-01-15T12:00:00Z"

    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/archive-slot"
    }
    clock.advance(5h)
    var early: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 1
        body: encoding.to_utf8("a,b\n")
    }
    assert early.status == 204
    clock.advance(2h)
    var late: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 2
        body: encoding.to_utf8("c,d\n")
    }
    assert late.status == 403
    assert late.code == "link_expired"
}

test "a plain slot's parts and part zero are refused" {
    var asked: Response = request {
        method: HttpMethod.post
        path: "/sheets/slot"
    }
    var plain: Response = request {
        method: HttpMethod.put
        upload: asked.output
        part: 1
        body: encoding.to_utf8("flour,1.20\n")
    }
    assert plain.status == 403
    assert plain.code == "link_invalid"
    var archive: Response = request {
        method: HttpMethod.post
        path: "/sheets/archive-slot"
    }
    var zero: Response = request {
        method: HttpMethod.put
        upload: archive.output
        part: 0
        body: encoding.to_utf8("a,b\n")
    }
    assert zero.status == 403
    assert zero.code == "link_invalid"
}
"#
            .to_string(),
        ),
    ];
    with_file(
        &mut plain,
        (
            "domains/bakery/entities/scan.jadpo",
            "Scan",
            "document",
            "max_size: 1kb\n        types: [FileType.pdf]",
        ),
        ("domains/bakery/actions/scan_slot.jadpo", "scan_slot"),
    );
    with_file(
        &mut plain,
        (
            "domains/bakery/entities/archive.jadpo",
            "Archive",
            "file",
            "max_size: 200mb\n        types: [FileType.csv]",
        ),
        ("domains/bakery/actions/archive_slot.jadpo", "archive_slot"),
    );
    with_file(
        &mut plain,
        (
            "domains/bakery/entities/attachment.jadpo",
            "Attachment",
            "file",
            "max_size: 1kb",
        ),
        (
            "domains/bakery/actions/attachment_slot.jadpo",
            "attachment_slot",
        ),
    );
    plain
        .into_iter()
        .map(|(path, text)| (path.to_string(), text))
        .collect()
}

fn project(name: &str, files: &[(String, String)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("jadpo-uploads-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let at = root.join(path);
        fs::create_dir_all(at.parent().expect("parent")).expect("folders");
        fs::write(at, text).expect("write");
    }
    root
}

/// The store folder a project's `app.jadpo` names; never inside the
/// project, which `jadpo check` would refuse.
fn store_folder(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("jadpo-uploads-store-{name}-{}", std::process::id()))
}

#[test]
fn uploads_pass_their_tests() {
    if !jadpo_test::live() {
        return;
    }
    let store = store_folder("interpreted");
    let files = files(&store.display().to_string());
    let root = project("interpreted", &files);
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let records = jadpo(&root, &["records", "update"]);
    assert_eq!(records.code, 0, "{}{}", records.stdout, records.stderr);
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&store);
}

type Got = (u16, Vec<(String, String)>, Vec<u8>);

/// How a request reaches a server: its method, path, headers and body,
/// and what came back.
type Send<'a> = &'a dyn Fn(&str, &str, &[(&str, &str)], &[u8]) -> Got;

fn header<'a>(got: &'a Got, name: &str) -> &'a str {
    got.1
        .iter()
        .find(|(header, _)| header == name)
        .map_or("", |(_, value)| value)
}

/// An upload over `send`, real HTTP to a real disk store at `store`, of the
/// bakery a project serves: a slot asked for at `/sheets/slot` (a field of
/// at most 64 bytes of `csv`), the file sent, the record attached at
/// `/sheets/attached`, and the file downloaded under the name the client
/// gave it; and each refusal a real client can see.
pub(crate) fn serves_an_upload(send: Send<'_>, store: &Path) {
    const BYTES: &[u8] = b"code\nA1\nB2\n";
    let asked = send("POST", "/sheets/slot", &[], b"");
    assert_eq!(asked.0, 200, "{asked:?}");
    let slot: serde_json::Value = serde_json::from_slice(&asked.2).expect("a slot");
    assert_eq!(slot["max_size"], serde_json::json!(64), "{slot}");
    let url = slot["url"].as_str().expect("a url").to_string();
    let token = slot["token"].as_str().expect("a token").to_string();
    assert!(url.starts_with("/files/sheets/uploads/upl_"), "{url}");
    assert!(url.ends_with(&token), "{url} {token}");

    // A compressed body is refused before anything is read.
    let refused = send("PUT", &url, &[("Content-Encoding", "gzip")], BYTES);
    assert_eq!(refused.0, 415, "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.2).contains("content_encoding_refused"),
        "{refused:?}"
    );
    // One that says it is over the limit is refused before it is sent.
    let over = send("PUT", &url, &[], &[b'a'; 200]);
    assert_eq!(over.0, 413, "{over:?}");
    // What came before them left nothing behind.
    assert_nothing_stray(store);

    let held = || {
        walk(store)
            .into_iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| !name.starts_with('.') && name != "link-key")
            })
            .collect::<Vec<_>>()
    };
    let before = held();
    let sent = send(
        "PUT",
        &url,
        &[(
            "Content-Disposition",
            "attachment; filename*=UTF-8''prices.csv",
        )],
        BYTES,
    );
    assert_eq!(sent.0, 204, "{sent:?}");
    // On the disk under its key, whole, the app's user's alone, with
    // nothing part-way left.
    let kept: Vec<PathBuf> = held()
        .into_iter()
        .filter(|path| !before.contains(path))
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(fs::read(&kept[0]).expect("kept"), BYTES);
    assert_nothing_stray(store);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&kept[0]).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    // A slot keeps one file, and a file never changes.
    let again = send("PUT", &url, &[], b"code\nC3\n");
    assert_eq!(again.0, 409, "{again:?}");
    assert_eq!(fs::read(&kept[0]).expect("kept"), BYTES);

    // Attached by an ordinary create, then downloaded under its name.
    let body = format!(r#"{{"supplier":"Millers","sheet":"{token}"}}"#);
    let attached = send(
        "POST",
        "/sheets/attached",
        &[("Content-Type", "application/json")],
        body.as_bytes(),
    );
    assert_eq!(attached.0, 201, "{attached:?}");
    let answer: serde_json::Value = serde_json::from_slice(&attached.2).expect("JSON");
    assert_eq!(answer["sheet"]["size"], serde_json::json!(BYTES.len()));
    assert_eq!(answer["sheet"]["name"], "prices.csv");
    let link = answer["sheet"]["url"].as_str().expect("a link").to_string();
    let back = send("GET", &link, &[], b"");
    assert_eq!(back.0, 200, "{back:?}");
    assert_eq!(back.2, BYTES);
    assert_eq!(
        header(&back, "content-disposition"),
        "attachment; filename*=UTF-8''prices.csv"
    );
    assert!(header(&back, "etag").starts_with('"'), "{back:?}");
}

/// A large file in parts over `send`, real HTTP to a real disk store at
/// `store`: a slot in parts asked for at `/sheets/archive-slot`, its first
/// part sent, the connection truly dropped part-way through the second,
/// which leaves nothing, that part resent, the record attached at
/// `/sheets/archived`, and the file downloaded whole.
pub(crate) fn serves_an_upload_in_parts(address: &str, send: Send<'_>, store: &Path) {
    use std::io::Write;
    const PART: usize = 16 * 1024 * 1024;
    const LAST: &[u8] = b"end,of,sheet\n";

    let asked = send("POST", "/sheets/archive-slot", &[], b"");
    assert_eq!(asked.0, 200, "{asked:?}");
    let slot: serde_json::Value = serde_json::from_slice(&asked.2).expect("a slot");
    assert_eq!(slot["part_size"], serde_json::json!(PART), "{slot}");
    assert_eq!(slot["max_size"], serde_json::json!(200_000_000), "{slot}");
    assert!(slot.get("url").is_none(), "{slot}");
    let parts = slot["parts_url"].as_str().expect("parts_url").to_string();
    let token = slot["token"].as_str().expect("a token").to_string();
    assert!(parts.ends_with(&format!("{token}/parts")), "{parts}");

    let first = vec![b'a'; PART];
    let sent = send(
        "PUT",
        &format!("{parts}/1"),
        &[(
            "Content-Disposition",
            "attachment; filename*=UTF-8''archive.csv",
        )],
        &first,
    );
    assert_eq!(sent.0, 204, "{:?}", (sent.0, &sent.1));

    // The connection goes away with most of the part unsent.
    {
        let mut stream = std::net::TcpStream::connect(address).expect("it accepts");
        write!(
            stream,
            "PUT {parts}/2 HTTP/1.1\r\nHost: {address}\r\nContent-Length: 1000\r\nConnection: close\r\n\r\nb,c\n"
        )
        .expect("sent");
        stream.flush().expect("flushed");
    }
    assert!(
        crate::disk_files::eventually(30, || {
            !walk(store).iter().any(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".part"))
            })
        }),
        "the dropped part left its temporary file behind"
    );

    let resent = send("PUT", &format!("{parts}/2"), &[], LAST);
    assert_eq!(resent.0, 204, "{resent:?}");

    let body = format!(r#"{{"supplier":"Millers","sheet":"{token}"}}"#);
    let attached = send(
        "POST",
        "/sheets/archived",
        &[("Content-Type", "application/json")],
        body.as_bytes(),
    );
    assert_eq!(attached.0, 201, "{attached:?}");
    let answer: serde_json::Value = serde_json::from_slice(&attached.2).expect("JSON");
    assert_eq!(
        answer["sheet"]["size"],
        serde_json::json!(PART + LAST.len())
    );
    assert_eq!(answer["sheet"]["name"], "archive.csv");

    // Joined whole, and the parts have gone.
    let parts_left: Vec<PathBuf> = walk(store)
        .into_iter()
        .filter(|path| {
            path.components().any(|part| {
                part.as_os_str()
                    .to_str()
                    .is_some_and(|name| name.ends_with(".parts"))
            })
        })
        .collect();
    assert!(parts_left.is_empty(), "{parts_left:?}");
    assert_nothing_stray(store);

    let link = answer["sheet"]["url"].as_str().expect("a link").to_string();
    let back = send("GET", &link, &[], b"");
    assert_eq!(back.0, 200, "{:?}", (back.0, &back.1));
    assert_eq!(back.2.len(), PART + LAST.len());
    assert!(back.2.starts_with(&first));
    assert!(back.2.ends_with(LAST));

    // The file never changes once joined.
    let again = send("PUT", &format!("{parts}/2"), &[], b"late\n");
    assert_eq!(again.0, 409, "{again:?}");
}

/// Every file below `at`.
fn walk(at: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut folders = vec![at.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                folders.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

/// No temporary file is left anywhere under `store`.
fn assert_nothing_stray(store: &Path) {
    let stray: Vec<PathBuf> = walk(store)
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".part"))
        })
        .collect();
    assert!(stray.is_empty(), "{stray:?}");
}

/// A chunked upload to a real server, its second chunk crossing the slot's
/// limit: the answer is `413` and nothing of it is kept on the disk.
pub(crate) fn a_chunked_upload_stops_at_its_limit(address: &str, send: Send<'_>, store: &Path) {
    use std::io::{Read, Write};
    let asked = send("POST", "/sheets/slot", &[], b"");
    let slot: serde_json::Value = serde_json::from_slice(&asked.2).expect("a slot");
    let url = slot["url"].as_str().expect("a url").to_string();
    let kept_before = walk(store).len();
    let mut stream = std::net::TcpStream::connect(address).expect("it accepts");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(20)))
        .expect("a timeout");
    write!(
        stream,
        "PUT {url} HTTP/1.1\r\nHost: {address}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
    )
    .expect("sent");
    let chunk = "a,b\n1,2\n".repeat(5);
    for _ in 0..3 {
        // A write past the answer's end may fail once it has been sent.
        if write!(stream, "{:x}\r\n{chunk}\r\n", chunk.len()).is_err() {
            break;
        }
    }
    let _ = stream.write_all(b"0\r\n\r\n");
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    assert!(answer.starts_with("HTTP/1.1 413"), "{answer}");
    assert!(answer.contains("file_too_large"), "{answer}");
    assert_nothing_stray(store);
    assert_eq!(walk(store).len(), kept_before, "nothing of it was kept");
}
