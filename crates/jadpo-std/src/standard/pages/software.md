# SemanticVersion

A software version: `1.2.3-alpha.1+build.5`.

Use it for your own or a package's release number. `<` compares two the way versions
are meant to compare. For a range such as `^1.2`, use `VersionRange`.

```jadpo
SemanticVersion { major: 1 minor: 2 patch: 3 pre_release: none build: none }
```

- `text.parse` reads `1.2.3`. A `v` at the front and `1.0` are refused, so give all
  three numbers.
- Build metadata, the part after `+`, is ignored when ordering.

# VersionRange

A range of versions: `^1.2.3`, `>=1.0.0 <2.0.0`.

Use it for what a dependency allows, in npm's grammar. For one version, use
`SemanticVersion`.

```jadpo
VersionRange("^1.2.3")
```

# GitRef

A git branch, tag, full ref or commit hash: `main`, `refs/tags/v1.2.0`.

Use it to record what a build came from. For a package, use `PackageUrl`.

```jadpo
GitRef("main")
```

- It follows git's own naming rules, so a name starting with `.` is refused.

# PackageUrl

A package's address, the same across package managers: `pkg:npm/left-pad@1.3.0`.

Use it in software inventories and security reports. For a version alone, use
`SemanticVersion`.

```jadpo
PackageUrl("pkg:npm/left-pad@1.3.0")
```

- The scheme and type are lower case, as the standard writes them.

# Cve

A security vulnerability's id: `CVE-2021-44228`.

Use it in security reports and advisories.

```jadpo
Cve("CVE-2021-44228")
```

- `CVE` is in capitals, and years before 1999 are refused.

# SpdxLicence

A software licence as SPDX writes it: `MIT OR Apache-2.0`.

Use it for a package's licence.

```jadpo
SpdxLicence("MIT OR Apache-2.0")
```

- Each name is spelled as the SPDX list has it, so `mit` and `Apache-2` are refused.

# TraceId

A whole trace's id: 32 lower-case hexadecimal digits, not all zero.

Use it to link logs across services. For one step in it, use `SpanId`. For the header
that carries both, use `TraceParent`.

```jadpo
TraceId("4bf92f3577b34da6a3ce929d0e0e4736")
```

# SpanId

One step's id in a trace: 16 lower-case hexadecimal digits, not all zero.

Use it beside a `TraceId`. For the header that carries both, use `TraceParent`.

```jadpo
SpanId("00f067aa0ba902b7")
```

# TraceParent

A `traceparent` header's value, which links a request to its trace.

Use it to pass a trace to or from another service, or read it from a request. For each
part alone, use `TraceId` and `SpanId`.

```jadpo
TraceParent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
```

# LogLevel

How severe a log record is: `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR` or `FATAL`.

Use it to filter or record logs.

```jadpo
LogLevel.warn
```

- `WARNING` and `CRITICAL` are refused, so change them first.

# FilePath

A portable file path: `/var/lib/acme/uploads`, `reports/2026/october.csv`.

Use it for a path in config. For a name with no folders, use `FileName`.

```jadpo
FilePath("reports/2026/october.csv")
```

- It uses `/` and its shortest form, so `./reports` and `a/../b` are refused.
- It doesn't check that the file exists.

# FileName

A file's name that every common file system takes: `report.pdf`.

Use it for a download's or an upload's name. For a whole path, use `FilePath`.

```jadpo
FileName("report.pdf")
```

- It refuses `/`, `\` and Windows' reserved names, and goes up to 255 bytes. So a name from an
  upload can't reach another folder.

# RegularExpression

A pattern for matching text: `[A-Z]{2}[0-9]{2}`.

Use it when a person sets a pattern while your app runs. A type's own `pattern:` is
checked when you compile, and suits a fixed rule better.

```jadpo
RegularExpression("[A-Z]{2}[0-9]{2}")
```

- Backreferences such as `\1` are refused, since patterns always run in linear time.

# ContainerImage

A container image's whole name: `docker.io/library/nginx:1.27`.

Use it for what a service runs. For a shipping container, use `ContainerNumber`.

```jadpo
ContainerImage("docker.io/library/nginx:1.27")
```

- It needs its registry, so `nginx:1.27` is refused.

# Digest

A content digest, with its algorithm: `sha256:` and 64 hexadecimal digits.

Use it to pin an image or a file. For a hash you compute in your app, use `Hash`.

```jadpo
Digest("sha256:6c3c624b58dbbcd3c0dd82b4c53f04194d1247c6eebdaab7c610cf7d66709b3b")
```

- The digits are lower case.

# Uuid

A universally unique identifier, in lower case with hyphens.

Use it as an id for records. Jadpo generates version 7 for a new record's `id`. For a
sortable id as shorter text, use `Ulid`.

```jadpo
Uuid("0192f0c4-9a1e-7b3c-8d2e-4f5a6b7c8d9e")
```

- It accepts any version. A type built on it can fix one with `version:`.

# Ulid

A sortable unique identifier: 26 capital letters and digits.

Use it when you receive ids in this form. For your own records, `Uuid` is what Jadpo
generates.

```jadpo
Ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV")
```

- It is upper case. The first character can't be above `7`.

# IdempotencyKey

A key a client sends so that a retried request isn't applied twice.

Use it on a route where a repeat must have no second effect, such as a payment. It runs
to 255 printable ASCII characters. For a record's own id, use `Uuid`.

```jadpo
IdempotencyKey("order-1042-attempt")
```

# Doi

A digital object identifier, naming a paper or a dataset: `10.1038/nphys1170`.

Use it to cite publications. For a person, use `Orcid`.

```jadpo
Doi("10.1038/nphys1170")
```

- It has no resolver in front, so `doi:` and `https://doi.org/` are refused.
