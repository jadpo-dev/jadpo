# Add search to a list
<!-- topic: guide-add-search-to-a-list -->

Search in Jadpo is one section on an entity. You name the fields people search,
and the compiler generates the index, keeps it up to date and writes its
migration, using PostgreSQL's full-text search, so there is nothing else to
run. Search results always pass policy, so nobody finds a record they couldn't
read. This guide adds a search box's route to a book catalogue.

You start from an app with a `Book` entity, its `BookView` output, and a
`POST /books` route that adds a book.
[Build an API](01-build-an-api.md) shows how to write those.

```jadpo check project=guide-search file=app.jadpo hidden
application {}
```

```jadpo check project=guide-search file=domains/books/types/new_book.jadpo hidden
use books/entities { Book }

/// What a caller sends to add a book.
type NewBook = Input {
    title: Book.title
    author: Book.author
}
```

```jadpo check project=guide-search file=domains/books/types/book_view.jadpo hidden
use books/entities { Book }

/// A book in a list.
type BookView = Output {
    pick: [Book.id, Book.title, Book.author]
}
```

```jadpo check project=guide-search file=domains/books/actions/add_book.jadpo hidden
use books/entities { Book }
use books/types { BookView, NewBook }

/// Adds a book to the catalogue.
action add_book(input: NewBook) -> BookView {
    var book: Book = Book.create {
        set: {
            title: input.title
            author: input.author
        }
    }
    return BookView {
        id: book.id
        title: book.title
        author: book.author
    }
}
```

## Say what is searched

`search:` lists the fields to search and the language they're written in.
Fields listed first count most when ordering the results, so a word in a title
beats the same word in an author's name. The language reduces each word to its
stem, so "marching" finds "march", and leaves out words such as "the".
`SearchLanguage.simple` does neither.

`domains/books/entities/book.jadpo`. `search:` is new:

```jadpo check project=guide-search file=domains/books/entities/book.jadpo
/// A book the library lends.
entity Book {
    id: Uuid
    title: Text {
        min_length: 1
        max_length: 200
    }
    author: Text {
        min_length: 1
        max_length: 100
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
    }

    search: {
        fields: [title, author]
        language: SearchLanguage.english
    }
}
```

The search needs a column and an index, and `jadpo migrate new` writes both:

```sh
jadpo migrate new add_book_search
```

```text
  compared the entities with 20261104T1120_initial

  Book
    + jadpo_search  tsvector        new search
    + index         (jadpo_search)  for Book

Wrote database/migrations/20261104T1121_add_book_search/
    snapshot.json  the whole schema at this point
    postgres.sql   2 statements: add column, create index

next: jadpo check  confirms the migration matches the entities
```

PostgreSQL works the column out from the fields on every write, so it can't
disagree with the record, and it fills it for the books you already have. There
is no backfill to write or index to keep up to date.

## Write the query

What someone types is a `SearchText`, a standard type holding 1 to 200
characters with no line breaks. `PageSize` and `PageCursor` are the standard
types for a page of results.

`domains/books/types/book_search.jadpo`:

```jadpo check project=guide-search file=domains/books/types/book_search.jadpo
/// What someone typed into the search box, and where its page starts.
type BookSearch = Input {
    phrase: SearchText
    page_size: PageSize {
        default: 10
    }
    after: PageCursor {
        optional: true
    }
}
```

A query finds records with `matches`. With no `order:`, the best matches come
first. It stands on a `where:` line of a list or a page, alone or joined to
other conditions by `and`.

`domains/books/queries/search_books.jadpo`:

```jadpo check project=guide-search file=domains/books/queries/search_books.jadpo
use books/entities { Book }
use books/types { BookSearch, BookView }

/// The books whose words match a search, best first.
query search_books(asked: BookSearch) -> Page<BookView> {
    from: Book
    where: Book matches asked.phrase
    page: {
        size: asked.page_size
        after: asked.after
    }
}
```

## Add the route

`domains/books/routes/books.jadpo`. The `GET /search` route and its imports are
new. On a `GET`, `input:` is read from the query string.

```jadpo check project=guide-search file=domains/books/routes/books.jadpo
use books/types { BookSearch, BookView, NewBook }
use books/actions { add_book }
use books/queries { search_books }

/// The library's catalogue.
routes Books {
    prefix: /books

    /// Adds a book.
    POST / {
        authentication: RouteAuthentication.none
        input: NewBook
        output: BookView
        status: SuccessStatus.created
        run: add_book(input)
    }

    /// The books matching what was typed, best first.
    GET /search {
        authentication: RouteAuthentication.none
        input: BookSearch
        output: Page<BookView>
        run: search_books(input)
    }
}
```

Run `jadpo api update` to bring the API record up to date.

## Test it

A test can check the order as well as the matches.

`domains/books/queries/search_books.test.jadpo`:

```jadpo check project=guide-search file=domains/books/queries/search_books.test.jadpo
use books/types { BookSearch, BookView, NewBook }
use books/actions { add_book }

test "a match in the title comes before a match in the author" {
    var by_eliot: BookView = add_book(
        NewBook {
            title: "Middlemarch"
            author: "George Eliot"
        }
    )
    var about_george: BookView = add_book(
        NewBook {
            title: "George and the Dragon"
            author: "Chris Wormell"
        }
    )
    var found: Page<BookView> = search_books(
        BookSearch {
            phrase: "george"
        }
    )
    assert list.map(found.items, item -> item.id) == [about_george.id, by_eliot.id]
    assert found.next == none
}

test "a word left out takes those books away" {
    var by_eliot: BookView = add_book(
        NewBook {
            title: "Middlemarch"
            author: "George Eliot"
        }
    )
    var about_george: BookView = add_book(
        NewBook {
            title: "George and the Dragon"
            author: "Chris Wormell"
        }
    )
    assert about_george.title == "George and the Dragon"
    var found: Page<BookView> = search_books(
        BookSearch {
            phrase: "george -dragon"
        }
    )
    assert list.map(found.items, item -> item.id) == [by_eliot.id]
}
```

```sh
jadpo test
```

```text
tested: 2 passed, 2 action tests
```

## Try it

Run `jadpo dev`, add a few books through `POST /books`, then search:

```sh
curl 'http://localhost:3000/books/search?phrase=george&page_size=2'
```

```text
{"items":[{"id":"01a1a64a-01a9-7647-9c27-6aca0a7908f9","title":"George and the Dragon","author":"Chris Wormell"},{"id":"01a1a64a-017c-73b2-889f-0660f9fa7817","title":"Middlemarch","author":"George Eliot"}],"next":"tcSu-eTxNPMIP88gOOAAAAADAaGmSgF8c7KInwZg-fp4Fw"}
```

`next` is the cursor for the following page, which a caller sends back as
`after`, and it is `null` on the last page. A `Page` holds no total.

A phrase is read as words, quoted phrases and `-word`, never as raw query
syntax, so whatever someone types can't break the query:

```sh
curl --get http://localhost:3000/books/search --data-urlencode 'phrase=george -dragon'
```

```text
{"items":[{"id":"01a1a64a-017c-73b2-889f-0660f9fa7817","title":"Middlemarch","author":"George Eliot"},{"id":"01a1a64a-0202-73b2-bf1e-700573be8dd3","title":"Silas Marner","author":"George Eliot"}],"next":null}
```

Every word has to match, and an `or` someone types is a word like any other. A
phrase of only words the language leaves out, such as "the", finds nothing.

## Who finds what

A search is a query, so policy filters its records in the same statement and
before the page is cut. A record a caller can't read is never found, never moves
one they can, and never changes where a page ends.

Each field in `fields:` must be one that everyone who may run the query may
read, because which records match shows what those fields say. Otherwise
[`policy.output_field_unproved`](../errors/policy.output_field_unproved.md) names
the `matches` line.

A field marked `personal:` can be searched. Its words stay in its record's own
row, so whatever clears the field clears them too. An encrypted field is never
searched.

## Read more

[Search](../language/16-built-in-services.md#search),
[Order and pages](../language/06-reads-and-writes.md#order-and-pages),
[When policy says no](../language/09-policy.md#when-policy-says-no) and
[Marking personal fields](../language/15-personal-data.md#marking-personal-fields).

## Not built yet

- Searching through an outside engine, such as Meilisearch or Algolia, isn't
  built yet: `store:` in `search:` is `toolchain.not_built_yet`. Use PostgreSQL's
  search, which needs no `store:`.
- `jadpo search reindex` comes with those engines, so it isn't built yet either.
  PostgreSQL's search has nothing to rebuild.
- Searching a field of a sensitive category, such as `PersonalData.health`, isn't
  built yet: it is `toolchain.not_built_yet` in `fields:`.
