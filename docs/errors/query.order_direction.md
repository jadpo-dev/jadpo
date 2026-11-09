# `query.order_direction`

An entry of a query's `order:` has no direction, or one it can't sort in.

## Why Jadpo refuses it

A sort has two directions, and a default would hide which one a query means,
so each entry of `order:` says it: `asc` from the smallest, `desc` from the
largest. A `geo.distance` order has one direction, nearest first, since only
the radius around a point bounds what the index reads.

## What it looks like

main.jadpo:

```jadpo check project=e-order-direction expect=query.order_direction
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf() -> List<Book> {
    from: Book
    order: [Book.title]
    limit: 20
}
```

```text output
error: `Book.title` says no direction
 --> main.jadpo:14:13
   |
14 |     order: [Book.title]
   |             ^^^^^^^^^^ `asc` or `desc`?
  reason: Each field of `order:` says which way it sorts: `asc` from the smallest, `desc` from the largest.
  next: Write `Book.title asc` or `Book.title desc` (agent)
  rule: query.order_direction
  help: https://jadpo.dev/docs/errors/query.order_direction

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An aggregate query's `order:` is the same, for the groups and totals it
sorts by:

main.jadpo:

```jadpo check project=e-order-direction-aggregate expect=query.order_direction
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
}

type TitleRow = Output {
    title: Title
    copies: Int
}

query title_rows() -> List<TitleRow> {
    from: Book
    group: {
        title: Book.title
    }
    aggregate: {
        copies: count()
    }
    order: [copies]
    limit: 20
}
```

```text output
error: `copies` says no direction
 --> main.jadpo:23:13
   |
23 |     order: [copies]
   |             ^^^^^^ `asc` or `desc`?
  reason: Each entry of `order:` says which way it sorts: `asc` from the smallest, `desc` from the largest.
  next: Write `copies asc` or `copies desc` (agent)
  rule: query.order_direction
  help: https://jadpo.dev/docs/errors/query.order_direction

check failed: 1 error, 1 file
```

`geo.distance` sorts nearest first only:

main.jadpo:

```jadpo check project=e-order-direction-distance expect=query.order_direction
type Reach = Length {
    min: 0metre
    max: 50km
}

entity Shop {
    id: Uuid
    spot: Point
}

query shops_near(doorstep: Point, reach: Reach) -> List<Shop> {
    from: Shop
    where: geo.within(Shop.spot, doorstep, distance: reach)
    order: [geo.distance(Shop.spot, doorstep) desc]
    limit: 20
}
```

```text output
error: `geo.distance` sorts nearest first only
 --> main.jadpo:14:13
   |
14 |     order: [geo.distance(Shop.spot, doorstep) desc]
   |             ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not `asc`
  reason: Records are sorted by distance only inside the radius a `geo.within(..., distance:)` line gives, which bounds what is read nearest first; farthest first has nothing to bound it (area 20, question 4).
  next: Write `asc` (agent)
  rule: query.order_direction
  help: https://jadpo.dev/docs/errors/query.order_direction

check failed: 1 error, 1 file
```

## How to fix it

**If the entry has no direction**, add one:

```jadpo fragment
    order: [Book.title asc]
```

```jadpo check project=e-order-direction-fix-1 hidden
type Title = Text {
    max_length: 80
}

entity Book {
    id: Uuid
    title: Title
    subtitle: Title?
    available: Bool
}

query shelf() -> List<Book> {
    from: Book
    order: [Book.title asc]
    limit: 20
}
```

Books now come back from A to Z. Write `desc` for the other way.

**If it sorts by distance**, write `asc`:

```jadpo fragment
    order: [geo.distance(Shop.spot, doorstep) asc]
```

```jadpo check project=e-order-direction-fix-2 hidden
type Reach = Length {
    min: 0metre
    max: 50km
}

entity Shop {
    id: Uuid
    spot: Point
}

query shops_near(doorstep: Point, reach: Reach) -> List<Shop> {
    from: Shop
    where: geo.within(Shop.spot, doorstep, distance: reach)
    order: [geo.distance(Shop.spot, doorstep) asc]
    limit: 20
}
```

The nearest shops come first, inside the radius the `where:` line gives.

## Related

- [Order and pages](../language/06-reads-and-writes.md#order-and-pages)
- [What can be sorted](../language/06-reads-and-writes.md#what-can-be-sorted)
