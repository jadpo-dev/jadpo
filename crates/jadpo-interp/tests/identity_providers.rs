//! Area 19, "Hashing and cryptography" (chunk 5.5r4), run: RS256 and
//! ES256 tokens verify against key sets shaped as Google's, Microsoft's
//! and Auth0's are, read with `json.read`; RFC 8725's attacks are refused
//! under both; a token's own claims are read as the application's type or
//! fail `mismatch` with the first problem; and `crypto.read_certificate`
//! reads a key's `x5c`.
//!
//! The tokens are signed with RFC 7515 §A.2's RSA key, by the id `rsa-1`,
//! and §A.3's P-256 key, by `ec-1`, around 2026-10-18T09:00:00Z, unless
//! the name says otherwise; `EVIL_CERT` holds an attacker's key.

use std::rc::Rc;

use crate::support;

use jadpo_interp::{Executor, Outcome, Program, Value};
use jadpo_runtime::{Clock, Ctx, Env, MemoryHost};
use support::{main, text, value};

const SOURCE: &str = "type KeySet = Object {\n    keys: List<Jwk>\n}\n\ntype KeySetText = Text {\n    max_length: 100000\n}\n\ntype CertificateText = Text {\n    max_length: 100000\n}\n\ntype Subject = Text {}\n\ntype KeySetUnreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TokenRefused = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TokenExpired = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype SignInUnexpected = Failure {\n    kind: FailureKind.invalid_value\n    internal: {\n        problem: DecodeProblem\n    }\n}\n\ntype NotACertificate = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype SignIn = Object {\n    email: Email\n    email_verified: Bool\n}\n\ntype Member = Object {\n    sub: Text\n    aud: List<Text>\n    exp: Instant\n    email: Email\n}\n\nfunction provider_keys(published: KeySetText) fails KeySetUnreadable -> List<Jwk> {\n    var key_set: KeySet = attempt json.read(published, as: KeySet) {\n        malformed: KeySetUnreadable\n        mismatch: KeySetUnreadable\n    }\n    return key_set.keys\n}\n\naction claims(token: Jwt, published: KeySetText) fails KeySetUnreadable, TokenRefused, TokenExpired -> JwtClaims {\n    var keys: List<Jwk> = attempt provider_keys(published)\n    return attempt crypto.verify_jwt(token, keys: keys, issuer: \"https://id.example.com\", audience: \"clinic\") {\n        invalid_signature: TokenRefused\n        expired: TokenExpired\n        not_yet_valid: TokenRefused\n        malformed: TokenRefused\n        wrong_audience: TokenRefused\n        wrong_issuer: TokenRefused\n    }\n}\n\naction sign_in(token: Jwt, published: KeySetText) fails KeySetUnreadable, TokenRefused, TokenExpired, SignInUnexpected -> SignIn {\n    var keys: List<Jwk> = attempt provider_keys(published)\n    return attempt crypto.verify_jwt(token, keys: keys, issuer: \"https://id.example.com\", audience: \"clinic\", as: SignIn) {\n        invalid_signature: TokenRefused\n        expired: TokenExpired\n        not_yet_valid: TokenRefused\n        malformed: TokenRefused\n        wrong_audience: TokenRefused\n        wrong_issuer: TokenRefused\n        mismatch: SignInUnexpected\n    }\n}\n\naction member(token: Jwt, published: KeySetText) fails KeySetUnreadable, TokenRefused, TokenExpired, SignInUnexpected -> Member {\n    var keys: List<Jwk> = attempt provider_keys(published)\n    return attempt crypto.verify_jwt(token, keys: keys, issuer: \"https://id.example.com\", audience: \"clinic\", as: Member) {\n        invalid_signature: TokenRefused\n        expired: TokenExpired\n        not_yet_valid: TokenRefused\n        malformed: TokenRefused\n        wrong_audience: TokenRefused\n        wrong_issuer: TokenRefused\n        mismatch: SignInUnexpected\n    }\n}\n\nfunction certificate_subject(x5c: CertificateText) fails NotACertificate -> Subject {\n    var der: Bytes = attempt encoding.from_base64(x5c) {\n        malformed: NotACertificate\n    }\n    var certificate: Certificate = attempt crypto.read_certificate(der) {\n        malformed: NotACertificate\n    }\n    return Subject(certificate.subject)\n}\n";

/// 2026-10-18T09:00:00Z.
const NOW: i64 = 1_792_314_000_000;

const RSA_N: &str = "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ";
const EC_X: &str = "f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU";
const EC_Y: &str = "x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0";
const RSA_GOOD: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.a-gfyWZTUFKEgxlMnq0ofX78bU2tIVyLxPyt2ggwIsODue5o4CFhEJAfwRWjIpCUeFAlOosx3uLMfsrIDtLfBosQ-3azQNWxhdSg6t5rBFLqKi6Hp2-ywT3scG9AiCWbR7gq57SEC3bMQzw7jrz4tMwjUUfgEMcn14_VjngNnbFYvV24ZNqrcxFBKqHJXrUVKxh79WgevMgicURa8toA99GWuq0VXSjVSYa789VRtwHelwyA1lUDtvimVWKl3WQh2cv4MTFhU5yDj8OEPT0GaKCJSpGp8pwXGn0pbfnI1e1noiDq02q8ckpvAhKypU6yEIv46NqA8nNhPqQ95FMgYw";
const EC_GOOD: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.t9RtrfYGqeVUUB7C2n9TofDzbl0SSaY-VTjSAydqG0BlYP_KEAWZNzs2aClILrxc2-i9wXyNSnzDP2GaVkRuQA";
const RSA_EXPIRED: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzEzOTY5LCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.iMiip0IrwCDvepOh5LiKBeGn1YiX_7m7U-AqG-EeIk5aNB3Gfola9QZ9RsJfOm_GIpnV2XtFo8RecX4ZXHrPHWqwJl4HqF2ua9eBs8TA10-HZBQXcm-TyabNWLeLqoHn_p5EMayNVhx6bBhUtf6idDUd_7oV5ULx9E7CKKynev-zil4KOFS-J894ZUetMl2gHUXWddDR1mkD3mCRcz77sTCM97q3JtKWqbBe9NRaZT56KYSal-AvL_u09-0oKu_Pv6YuhsVtm88_stN5QTakweiR2MifU8CBJ0TuTheMsm61VGQ7T3hLCQk4Oz-J4d2loxeDCMrJHwbr21l8V0WICw";
const EC_EXPIRED: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzEzOTY5LCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.OPybO4zZlnSREk3n5c6T_-nd3tm3Bu1mp4NnNbuQVd1MQXuyV00-1IY3HoiYy1pJuOGlN_UN4hrRGNl5k6MIWw";
const RSA_TAMPERED: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTEiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMiLCJlbWFpbCI6ImFkYUBleGFtcGxlLmNvbSIsImVtYWlsX3ZlcmlmaWVkIjp0cnVlLCJhenAiOiJjbGluaWMtd2ViIn0.a-gfyWZTUFKEgxlMnq0ofX78bU2tIVyLxPyt2ggwIsODue5o4CFhEJAfwRWjIpCUeFAlOosx3uLMfsrIDtLfBosQ-3azQNWxhdSg6t5rBFLqKi6Hp2-ywT3scG9AiCWbR7gq57SEC3bMQzw7jrz4tMwjUUfgEMcn14_VjngNnbFYvV24ZNqrcxFBKqHJXrUVKxh79WgevMgicURa8toA99GWuq0VXSjVSYa789VRtwHelwyA1lUDtvimVWKl3WQh2cv4MTFhU5yDj8OEPT0GaKCJSpGp8pwXGn0pbfnI1e1noiDq02q8ckpvAhKypU6yEIv46NqA8nNhPqQ95FMgYw";
const EC_TAMPERED: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTEiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMiLCJlbWFpbCI6ImFkYUBleGFtcGxlLmNvbSIsImVtYWlsX3ZlcmlmaWVkIjp0cnVlLCJhenAiOiJjbGluaWMtd2ViIn0.t9RtrfYGqeVUUB7C2n9TofDzbl0SSaY-VTjSAydqG0BlYP_KEAWZNzs2aClILrxc2-i9wXyNSnzDP2GaVkRuQA";
const RSA_ALG_NONE: &str = "eyJhbGciOiJub25lIiwia2lkIjoicnNhLTEifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.a-gfyWZTUFKEgxlMnq0ofX78bU2tIVyLxPyt2ggwIsODue5o4CFhEJAfwRWjIpCUeFAlOosx3uLMfsrIDtLfBosQ-3azQNWxhdSg6t5rBFLqKi6Hp2-ywT3scG9AiCWbR7gq57SEC3bMQzw7jrz4tMwjUUfgEMcn14_VjngNnbFYvV24ZNqrcxFBKqHJXrUVKxh79WgevMgicURa8toA99GWuq0VXSjVSYa789VRtwHelwyA1lUDtvimVWKl3WQh2cv4MTFhU5yDj8OEPT0GaKCJSpGp8pwXGn0pbfnI1e1noiDq02q8ckpvAhKypU6yEIv46NqA8nNhPqQ95FMgYw";
const EC_ALG_NONE: &str = "eyJhbGciOiJub25lIiwia2lkIjoiZWMtMSJ9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.t9RtrfYGqeVUUB7C2n9TofDzbl0SSaY-VTjSAydqG0BlYP_KEAWZNzs2aClILrxc2-i9wXyNSnzDP2GaVkRuQA";
const RSA_HS256_PUBLIC_KEY: &str = "eyJhbGciOiJIUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.EsNP8oGfkbz3bTQAGTA06RNXvHCRBc5QFbPsEEJi4LM";
const EC_HS256_PUBLIC_KEY: &str = "eyJhbGciOiJIUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.A_e1k62suuCvcp2YwcqJgES-BC7e7PBYJ1SQO7i3vbA";
const RSA_SIGNED_EC_KID: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6ImVjLTEifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.ZrJYeDnIAWdpUEiAQib_QHQgpEEiv726xbD4jjazvDcUr62oFjN2ZWb65f7OoP4SLQUv93q-l8r4dx6wB_o5PsZP2jlNeW38X9reEIcalFtDh0D_x7hW03yDzwBKfiif0ewwp8qtTFvA9etFsrO2wjUY1Fyqim6JXaTxQo9-Y3tZU1gKtPgc-bONVak6LxGdh1sw3BobpEG40KuL7NpjZKBpJC0vuMai9OHvqPG-iSOKtLrQXYcm4LZjM6TNghjdq05sGrNJ6R0K-tXUpr6rrrzniPStnS_quQrGSqSmRS9HaccRlA7LFjQyC0GQ0RuFpxeW0UqpP6dJsbgrLINUDQ";
const EC_SIGNED_RSA_KID: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6InJzYS0xIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.DedL0SvCvju1My8cy8MLvVbzP9Um-zH6R6RBcd4L3C0iE-24u8LOjfWrMIUsYmd4k37v0rGivYkGo8Z8C-QJkg";
const RSA_EMBEDDED_JWK: &str = "eyJhbGciOiJSUzI1NiIsImp3ayI6eyJrdHkiOiJSU0EiLCJuIjoick9wYjNTbXNhdGl4ZGZvME9pUmlZQ1lxYlhSSXpRWlhnRkJncS1GQ1B6dndxQkJQZDNIOEZnMmVJMnBJbE9GeW4yUHpIaWNTRGMzZnpDTFJiSHFNelhCYk1GM0ZLUF9sSHltbzJZZjlmQUpPZUZ0elRhQVRKelJUVTBVTDhZWjZCbVhVVXhvV21LYW1uMnJaVzZnWXE3bVM1UjRUcDVaVzFLeGpHdWhYU1BvRy1rTm10YnF0WU81NElxVllodkgzeU03LVJGZWtybS04MnhudGNVSjBGUmpHX01YV1FlXzNKTDlqbHRlbDFEQ25Zb3dfaFhHWmZrSktja2F4YjVLNkw1QVpRWm1melVkU1BkOEFORmhiaUJZQm5kZi1UelhxRmNub1Bwb3dPTVRUdlBxNlNVdzFjMUhEcUlNYnpmRlJtRzlxcFE1OTVmTkZEdFU1RTFTam93IiwiZSI6IkFRQUIifX0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.Q6C-GgUc9ybRTYram8A4KNPQ95OTyliHJDsC8bbKIeOsG1I0N5cUoAnD6vWzH25cMiWUjxt_zCuruRkEfjUSsKJdJOtqfl_qUT-r8ta6UoPpRxGYl4YoGR0N3swPC-206cx7LdALDv-WMk6ovzbcVw4QdsgIn5DNy_Ckfh_OiZDgE9x_DmJYqIUFNh-RAvmYadSxBTzwtoGYdYbQB6kyJFSG-UG69MszvXywMG0VoKH7493MHQRsTdOxljowuuLPylDsmjZIxVbB-YlbVkdR3EGrpBlzYxFwlXollyRicMnyQHdr6_Rq96lNZGceVo5Id0QyiYbUvX8djzmcusC94Q";
const EC_EMBEDDED_JWK: &str = "eyJhbGciOiJFUzI1NiIsImp3ayI6eyJrdHkiOiJFQyIsImNydiI6IlAtMjU2IiwieCI6IldmNldacGMyMjQ3ZnVoQk5rT202UldlTl96Ykp3MjRlYTdyVERDVFVhMlEiLCJ5IjoiRzVKbmRSRU1UR0FtRE9JSDFMMWIyZVdxTExUZ3NOalpWREZFSFVLZGNtQSJ9fQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.CKhilASsqJUPc_0UGEErXN8oeEC3NpEgIjNpeKgFXhqBZ1yj4JayjMk7cXHKf_H3Xw_PyxQt8azdyacsNrRZow";
const RSA_JKU: &str = "eyJhbGciOiJSUzI1NiIsImprdSI6Imh0dHBzOi8vZXZpbC5leGFtcGxlLmNvbS9rZXlzIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.AcNmxrppfCh11L4-IyYxoPkLLXhZgyhPWHkOMrzkcvZUiaR9o4tq3p4oxXUPL1Ujk2XW85DHCgn1IeTmI4tfU5-yAwkW8GdNq_JoBlebu7NDweDE_Hejgzn7efbbA6r4EQPPpg3rdWRp2Fx7Vu2Gw2ycttN9pfUACx12DXPJRlt-mgEVlbhBQ8tRH_DybxPN-rzl8Q51kFmq7NcoNF4JeMcqKV4k9Zt513QnRgK_H0Kbhuf1UCAcc47WgDk5zZAW5g0HspP5mKn-hNwywf91BSJgyI-x8EBo40HbHh-WwkEW48vbKnz73H5C_AcEiMwOxlgUYGyITdCmapQ7NCAgyA";
const EC_JKU: &str = "eyJhbGciOiJFUzI1NiIsImprdSI6Imh0dHBzOi8vZXZpbC5leGFtcGxlLmNvbS9rZXlzIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.O5n3GAQ6kiTBKVD900xtePKC8HHjVZ_ds1blRpuszAPQ15BmqPufwnJZ_vf9RK3y8KPylZZILqZmCmbokmR7TA";
const RSA_CRIT: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwiY3JpdCI6WyJleHAiXSwiZXhwIjoxNzkyMzE3NjAwfQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.lmOHPv8hmh96heQx_2jBi5qIxT6GAw6_x7z8fdQFE359sq1OrW7GnAvHnUfMEKFlZfuN7GR7G5Mhag6pyTsLizOUEciU3of5KBTJEXGaYkevCeCUF3njSlzPbCst1m0ejRN8YauG7F9SharyZpcmi8X-_otrGUXrUgyGUFu1pW6K-SByQWMai-ddfWeQwlpHDixJ6-1_Jva1M5n5ZpgDpC9u-peQisDDTj4Wl1rQBoROPA-BSi_Sq8kxau8gKRjnZBTJEV6CEcezktaNwUGX0WlzophNaE2znDzaLzr33Ueek3AuiDI-EOHYRqAL2jo0QrRuTR5HbqLk_GndYxCboA";
const EC_CRIT: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJjcml0IjpbImV4cCJdLCJleHAiOjE3OTIzMTc2MDB9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.AnQvQk-FgwHESNpeeTW5Sh6h8tJP7yFJ46J1Mp02JwGwJZkiiBg0Nq7YabnTKmDfI09ZpaqrxwlsHiKto29zMg";
const RSA_DUPLICATE_SUBJECT: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiIsInN1YiI6Im1lbWJlci0xIn0.gLlnrGftDriLHBKVhrBXwaXAgZgl7KAGUPjcMQifdTmQiRv_NCgBaZ-zEx8tYoT3alFj_y2HtQtyrIYYsO2N9DEtSAhfpuRh6NzwDO7PDtSTg0ICaS4cgr4Ftsbn-69-yTIGKWFjVa6G6cW1pxiaOCbB6q-Xlj--EiIDMz8FhkzxBSPNKzveXPPguK40RdT9sDzdzsWv0zniG9jrTI-TpymQvj6gwjwP32vXnvtqPySn2WvpF1M6LZHF_ZXu3xSNnw3qL-nj_NVtXnTW4YlVqlyN-5tcGSD12XX60xsd5ZWK4Z6huwGbJ1p1UBwGGq3VIG0yYPceY4FpgOabgZJQSQ";
const EC_DUPLICATE_SUBJECT: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiIsInN1YiI6Im1lbWJlci0xIn0.99ipXDg95m1LBHQCi-4N3E6D1VXkEpUkCC6tPr3mjC5h0PECGVEK6IbdtxfLBXX__jJUkARR051gZXJ8qoBoLA";
const RSA_OTHER_AUDIENCE: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoicGhhcm1hY3kiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.nVnE89EGV9S3LVtGcpFZeYj1Ge6_ktUEIK80AJA0y7qTzFmY8aLSQTQeTQ3d_bOnaivYXYG9-NK8XVm_mIbwHkn1Ic3RNspK3_8ZRRkNlXKMHhOFnTowyHQOtl-g15FRHFl586tHS887V99D9AMcIfUuNJwqrND5u89x2IlgGAoL2MYMhVJu8rtx7Y1I5yLFUcewO6p-esby8TvRHOYgQUcyVSA0GnsVLP67O58uJCCjnmvQCDDxuXsb7HJ6lJe2iFKCcFR0LGHH7hsyrCrQmREjhc0LkeMi3JKokPxWAd84SkDa8Izd27Iav9GiTyU__nQzPQUbwHxP3cFAV-v1iw";
const EC_OTHER_ISSUER: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2V2aWwuZXhhbXBsZS5jb20iLCJzdWIiOiJtZW1iZXItNDIiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.WI5PvaRXiMMAq5uPk_pqkAVH02JT2SQf5ZTJTJctUBR1jwuSpOj99BAyXPRgEqEdyZA79bHvsbc4nYoTlvGaMw";
const RSA_EMAIL_NOT_A_FLAG: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6InllcyIsImF6cCI6ImNsaW5pYy13ZWIifQ.QNbt57Bah-sLVNeOfEL4yMyNmUdJmNSUPkDyebskCF_7ai6bJCkgGLaR3mtkuR2NnwYM8RTJYKws0LAHbCojPhqCj8f2h_MuM51YnbVzxhmVM13_IbdYYIqv1SxIQczETo-t_3MVVoLb6mewLo_Ma4x4eJQ8P9Q2jxa552EfwzVbCvqXsK39V3qUa5Qiju0eTfnJaY2VVRGQa5FUKoHU901FAONaQhMm6h7_O7yJ8SCHqoP7nbRqkizUftnrntHIyVe3ggwgr2eU6UOW3WcnkUKpWyYFlys7p22xXgvfODUgJickJYQAwsew1XcFAidXDVQzFV0NrNRil5ZYBLjkgw";
const EC_NO_EMAIL: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.K612c1Htjg5APrJ297tfbFon6kOz7tSAjQ1ZuUCSf1uZD2uftvt4w-X9khNWOAhQ-5AiUEb7Wr3TRqIo751dZA";
const RSA_CERT: &str = "MIIDCzCCAfOgAwIBAgIEHy49TDANBgkqhkiG9w0BAQsFADAdMRswGQYDVQQDDBJqYWRwby1qd2tzLmV4YW1wbGUwHhcNMjYxMDAxMDAwMDAwWhcNMzYxMDAxMDAwMDAwWjAdMRswGQYDVQQDDBJqYWRwby1qd2tzLmV4YW1wbGUwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCh+BYK4uPJtGXOjS1lYmM2K5J9vinh8CR3/BYlzJChNuOL2TSXxbbqY913EeZ8dCn5VrD7io8ImtxLaYk8wTM/U+3QGbh3hCUv7JFP5IV3aVlL6kKA0ywPVb9ilE8TA5a8bpvfbr3SvaNnjuygxmj3AbONv/s4yDQs4v5tJ/reSlpIdJed1LnPmt7Ex1sFhSwsD174pcF1A5L5ROjtZMEQxrZHYJqkeDrrnGya11UxMFBji4NmXG9veoKjlnAqH2QbgtPr8jkiGUkftoaHLFcW9Qr4NY2ai50Xw0Byj3+H2JoY2PyrZ62EWQwuz3WTOTY8BwNNb2BvniHgVFbK5emhAgMBAAGjUzBRMB0GA1UdDgQWBBQzGIiPp7+DkM3e011UielRYxna2TAfBgNVHSMEGDAWgBQzGIiPp7+DkM3e011UielRYxna2TAPBgNVHRMBAf8EBTADAQH/MA0GCSqGSIb3DQEBCwUAA4IBAQB4Wur8KUXryaWEVRE2RxcMV6KlJ5YyznKsfUZJcEBAB9woW1MEol8LHFz1hIyB6nQYAul25kaAhIrCh0OHZeix2GkrmFHmDvNPFoOXu/hSBSI/XxFCoUCY3QsUaHoWNqN/VBmjpMiaOGYO5G/LV80uG4/8w2pdG/C96PWkInb+iLl1n+0gt4uOlfsJeLv941A5s24zhT1gVlhucKo38HxGfugjT9C4l1mTo4Hiu/lG/gEgmEM6NF7pbymasTYDmRx6kI31Zb/Ai6ELBeIfokOvvCUNGFsTnK5+bZtiq1yGfly8ahVWYh34h3bpLSi89/eNAHa/rwYUbGvi/nbmAaPx";
const EVIL_CERT: &str = "MIIDCzCCAfOgAwIBAgIEHy49TDANBgkqhkiG9w0BAQsFADAdMRswGQYDVQQDDBJqYWRwby1qd2tzLmV4YW1wbGUwHhcNMjYxMDAxMDAwMDAwWhcNMzYxMDAxMDAwMDAwWjAdMRswGQYDVQQDDBJqYWRwby1qd2tzLmV4YW1wbGUwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCs6lvdKaxq2LF1+jQ6JGJgJiptdEjNBleAUGCr4UI/O/CoEE93cfwWDZ4jakiU4XKfY/MeJxINzd/MItFseozNcFswXcUo/+UfKajZh/18Ak54W3NNoBMnNFNTRQvxhnoGZdRTGhaYpqafatlbqBiruZLlHhOnllbUrGMa6FdI+gb6Q2a1uq1g7ngipViG8ffIzv5EV6Sub7zbGe1xQnQVGMb8xdZB7/ckv2OW16XUMKdijD+FcZl+QkpyRrFvkrovkBlBmZ/NR1I93wA0WFuIFgGd1/5PNeoVyeg+mjA4xNO8+rpJTDVzUcOogxvN8VGYb2qlDn3l80UO1TkTVKOjAgMBAAGjUzBRMB0GA1UdDgQWBBS5i01sJa2jd1jR9aZ4KA4rcIhjzDAfBgNVHSMEGDAWgBS5i01sJa2jd1jR9aZ4KA4rcIhjzDAPBgNVHRMBAf8EBTADAQH/MA0GCSqGSIb3DQEBCwUAA4IBAQAEz30hZw31MjGmt9jTq1reKEUrgMmn4uvB1ZkmyTqx3aP/D9beHsa/eXOt3SesWFbGNtaGy9TnugCbaPX9iQCWpuN1XTQ9VjW09P7O5voehwHjhkM+u1QXs6uS68VQ2n1L5gUQTN/TIU/skEwFEIbsq/RklGHe8f3Y58tlxMtjivfkC0A0lqJ47Z3lUfqKmvcWRs0mUX+px9pSH8AzI3T/HyIYFdrlEfU9slQDWjrhzvjBdbly4b0BYaVO5x5uWcAYnJNvkHgv3naWLtyXZ6GR4g8SLKh19G0Fp+mG5tKMAjOFOTyHta2CCKx9y6P8H2bfdCjQ/IvohyWhSe303gRL";

/// Google's shape: `e` first, `alg` and `use` on each key, no certificates.
fn google() -> String {
    format!(
        r#"{{"keys":[{{"e":"AQAB","n":"{RSA_N}","use":"sig","kid":"rsa-1","alg":"RS256","kty":"RSA"}},{{"kty":"EC","crv":"P-256","x":"{EC_X}","y":"{EC_Y}","use":"sig","kid":"ec-1","alg":"ES256"}}]}}"#
    )
}

/// Microsoft's shape: no `alg`, a thumbprint, the certificate, and members
/// of its own, which are ignored.
fn microsoft(certificate: &str) -> String {
    format!(
        r#"{{"keys":[{{"kty":"RSA","use":"sig","kid":"rsa-1","x5t":"qmVSyBLf2W5xPbZT2b2Ofl7RUl8","n":"{RSA_N}","e":"AQAB","x5c":["{certificate}"],"cloud_instance_name":"microsoftonline.com","issuer":"https://login.microsoftonline.com/9188040d-6c67-4c5b-b112-36a304b66dad/v2.0"}}]}}"#
    )
}

/// Auth0's shape: `alg` first, a thumbprint and the certificate.
fn auth0() -> String {
    format!(
        r#"{{"keys":[{{"alg":"RS256","kty":"RSA","use":"sig","n":"{RSA_N}","e":"AQAB","kid":"rsa-1","x5t":"qmVSyBLf2W5xPbZT2b2Ofl7RUl8","x5c":["{RSA_CERT}"]}}]}}"#
    )
}

/// `name(token, published)` at `now`.
fn called(program: &Program, name: &str, token: &str, published: &str, now: i64) -> Outcome {
    let id = program.plan().find(name).expect(name);
    let host = MemoryHost::new();
    let mut ctx = Ctx::of(
        program,
        &host,
        Env {
            seed: Some(7),
            clock: Clock::fixed(now),
        },
    );
    jadpo_runtime::block_on(program.run(
        &mut ctx,
        id,
        vec![Value::text(token), Value::text(published)],
    ))
    .expect("ran")
}

fn failed(program: &Program, name: &str, token: &str, published: &str) -> String {
    match called(program, name, token, published, NOW) {
        Outcome::Failed(failure) => failure.name.clone(),
        other => panic!("{token}: {other:?}"),
    }
}

#[test]
fn google_s_microsoft_s_and_auth0_s_key_sets_verify_their_tokens() {
    let program = main(SOURCE);
    for (token, published) in [
        (RSA_GOOD, google()),
        (EC_GOOD, google()),
        (RSA_GOOD, microsoft(RSA_CERT)),
        (RSA_GOOD, auth0()),
    ] {
        let Outcome::Value(Value::Record(claims)) =
            called(&program, "claims", token, &published, NOW)
        else {
            panic!("{token} under {published}");
        };
        assert_eq!(claims.get("sub"), Some(&Value::text("member-42")));
        assert_eq!(
            claims.get("aud"),
            Some(&Value::list(vec![Value::text("clinic")]))
        );
    }
    // An hour and a minute on, each has expired.
    match called(&program, "claims", EC_GOOD, &google(), NOW + 3_660_000) {
        Outcome::Failed(failure) => assert_eq!(failure.name, "TokenExpired"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_key_set_holding_a_secret_or_a_key_nothing_checks_with_is_unreadable() {
    let program = main(SOURCE);
    let private = google().replace(r#""kty":"RSA""#, r#""kty":"RSA","d":"AQAB""#);
    let shared = r#"{"keys":[{"kty":"oct","k":"GawgguFyGrWKav7AX4VKUg"}]}"#;
    let restricted = google().replace(r#""kty":"RSA""#, r#""kty":"RSA","key_ops":["verify"]"#);
    let off_curve = google().replace(EC_Y, &format!("{}4", &EC_Y[..42]));
    let confused = google().replace(r#""alg":"RS256""#, r#""alg":"ES256""#);
    for published in [private, shared.to_string(), restricted, off_curve, confused] {
        assert_eq!(
            failed(&program, "claims", RSA_GOOD, &published),
            "KeySetUnreadable",
            "{published}"
        );
    }
}

#[test]
fn the_attack_suite_is_refused_under_both_algorithms() {
    let program = main(SOURCE);
    for (token, refused) in [
        (RSA_ALG_NONE, "TokenRefused"),
        (EC_ALG_NONE, "TokenRefused"),
        (RSA_HS256_PUBLIC_KEY, "TokenRefused"),
        (EC_HS256_PUBLIC_KEY, "TokenRefused"),
        (RSA_SIGNED_EC_KID, "TokenRefused"),
        (EC_SIGNED_RSA_KID, "TokenRefused"),
        (RSA_EXPIRED, "TokenExpired"),
        (EC_EXPIRED, "TokenExpired"),
        (RSA_OTHER_AUDIENCE, "TokenRefused"),
        (EC_OTHER_ISSUER, "TokenRefused"),
        (RSA_TAMPERED, "TokenRefused"),
        (EC_TAMPERED, "TokenRefused"),
        (RSA_EMBEDDED_JWK, "TokenRefused"),
        (EC_EMBEDDED_JWK, "TokenRefused"),
        (RSA_JKU, "TokenRefused"),
        (EC_JKU, "TokenRefused"),
        (RSA_CRIT, "TokenRefused"),
        (EC_CRIT, "TokenRefused"),
        (RSA_DUPLICATE_SUBJECT, "TokenRefused"),
        (EC_DUPLICATE_SUBJECT, "TokenRefused"),
    ] {
        assert_eq!(
            failed(&program, "claims", token, &google()),
            refused,
            "{token}"
        );
    }
    // A key whose certificate holds another key is never tried.
    assert_eq!(
        failed(&program, "claims", RSA_GOOD, &microsoft(EVIL_CERT)),
        "TokenRefused"
    );
}

#[test]
fn a_token_s_own_claims_are_read_as_the_application_s_type() {
    let program = main(SOURCE);
    for token in [RSA_GOOD, EC_GOOD] {
        let Outcome::Value(Value::Record(sign_in)) =
            called(&program, "sign_in", token, &google(), NOW)
        else {
            panic!("{token}");
        };
        assert_eq!(sign_in.get("email"), Some(&Value::text("ada@example.com")));
        assert_eq!(sign_in.get("email_verified"), Some(&Value::Bool(true)));
    }
    // Claims that aren't the type fail `mismatch`, its first problem
    // passed on, never the value.
    let problem = |field: &str, rule: &str| {
        Value::record(
            "DecodeProblem",
            vec![
                (Rc::from("field"), Value::text(field)),
                (Rc::from("rule"), Value::text(rule)),
            ],
        )
    };
    for (token, field, rule) in [
        (RSA_EMAIL_NOT_A_FLAG, "email_verified", "type"),
        (EC_NO_EMAIL, "email", "required"),
    ] {
        match called(&program, "sign_in", token, &google(), NOW) {
            Outcome::Failed(failure) => {
                assert_eq!(failure.name, "SignInUnexpected");
                assert_eq!(
                    failure.internal,
                    [("problem".to_string(), problem(field, rule))]
                );
            }
            other => panic!("{token}: {other:?}"),
        }
    }
    // The token is checked first: a tampered one is refused, never read.
    assert_eq!(
        failed(&program, "sign_in", RSA_TAMPERED, &google()),
        "TokenRefused"
    );
    assert_eq!(
        failed(&program, "sign_in", EC_EXPIRED, &google()),
        "TokenExpired"
    );
}

#[test]
fn one_call_reads_the_subject_and_the_email() {
    let program = main(SOURCE);
    for token in [RSA_GOOD, EC_GOOD] {
        let Outcome::Value(Value::Record(member)) =
            called(&program, "member", token, &google(), NOW)
        else {
            panic!("{token}");
        };
        // The registered claims as `JwtClaims` holds them, checked first.
        assert_eq!(member.get("sub"), Some(&Value::text("member-42")));
        assert_eq!(
            member.get("aud"),
            Some(&Value::list(vec![Value::text("clinic")]))
        );
        assert_eq!(member.get("exp"), Some(&Value::Instant(NOW + 3_600_000)));
        assert_eq!(member.get("email"), Some(&Value::text("ada@example.com")));
    }
    // A token without an email is `mismatch`; a tampered subject never
    // reaches the type.
    match called(&program, "member", EC_NO_EMAIL, &google(), NOW) {
        Outcome::Failed(failure) => assert_eq!(failure.name, "SignInUnexpected"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        failed(&program, "member", RSA_TAMPERED, &google()),
        "TokenRefused"
    );
}

#[test]
fn a_key_s_certificate_is_read_from_its_der() {
    let program = main(SOURCE);
    assert_eq!(
        value(&program, "certificate_subject", vec![text(RSA_CERT)]),
        text("CN=jadpo-jwks.example")
    );
    match support::run(&program, "certificate_subject", vec![text("AAAA")]) {
        Outcome::Failed(failure) => assert_eq!(failure.name, "NotACertificate"),
        other => panic!("{other:?}"),
    }
}
