/// The courier's package, at `sandbox`, and a shipping domain that keeps
/// shipments.
pub fn files(sandbox: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "app.jadpo",
            r#"application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
        }
    }
}
"#
            .to_string(),
        ),
        (
            "config.jadpo",
            r#"config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/package.jadpo",
            r#"/// Acme Couriers' booking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/provider.jadpo",
            format!(
                r#"/// Acme Couriers' API.
provider {{
    protocol: Protocol.json_http
    hosts: {{
        live: "https://api.acme-couriers.example"
        sandbox: "{sandbox}"
    }}
    credential: CredentialScheme.bearer
    idempotency: {{
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
        in_use: "key_in_use"
    }}
    error_code: "error.code"
}}
"#
            ),
        ),
        (
            "shared/providers/acme_couriers/v1/resources/bookings.jadpo",
            r#"public resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        output_at: "data"
        changes: true
        idempotent: true
        failures: [SlotTaken]
        source: PackageSource.written
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/create_booking.jadpo",
            r#"/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
    slot_id: SlotId {
        wire_name: "slotId"
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/booking.jadpo",
            "/// A booked collection.\npublic type Booking = Object {\n    id: Text\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/postcode.jadpo",
            "/// A postcode as the courier takes it.\npublic type Postcode = Text {\n    max_length: 10\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/slot_id.jadpo",
            "/// A collection slot's code.\npublic type SlotId = Text {\n    max_length: 10\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/failures/slot_taken.jadpo",
            r#"/// The collection slot was taken first.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "The collection slot is no longer available."
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/types/note.jadpo",
            "/// What the warehouse wrote about a shipment.\ntype Note = Text {\n    max_length: 40\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/types/label.jadpo",
            "/// What is printed on a shipment.\ntype Label = Text {\n    max_length: 80\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/failures/collection_slot_gone.jadpo",
            "type CollectionSlotGone = Failure {\n    kind: FailureKind.conflict\n    message: \"That collection slot has gone.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/failures/label_invalid.jadpo",
            "type LabelInvalid = Failure {\n    kind: FailureKind.invalid_value\n    message: \"The label is too long to print.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/entities/shipment.jadpo",
            r#"use shipping/types { Label, Note }

/// A parcel waiting for collection.
entity Shipment {
    id: Uuid
    note: Note
    label: Label?

    policy: {
        create: [Access.public, Access.system]
        read: [Access.public, Access.system]
        update: [Access.public, Access.system]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/entities/pickup.jadpo",
            r#"use shipping/entities { Shipment }

/// A collection booked for a shipment.
entity Pickup {
    id: Uuid
    shipment_id: Shipment.id {
        on_delete: OnDelete.restricted
    }
    booking: Text

    policy: {
        create: [Access.public, Access.system]
        read: [Access.public, Access.system]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/book_pickup.jadpo",
            r#"use app { couriers }
use shipping/entities { Pickup, Shipment, ShipmentNotFound }
use shipping/failures { CollectionSlotGone }
use shared/providers/acme_couriers

/// Reads a shipment, books its collection, then keeps a pickup that
/// refers to the shipment it read.
action book_pickup(shipment_id: Shipment.id, postcode: AcmeCouriers.Postcode, slot: AcmeCouriers.SlotId) fails ShipmentNotFound, CollectionSlotGone -> Pickup {
    var shipment: Shipment = attempt Shipment.get(shipment_id)
    var booking: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: postcode
        slot_id: slot
    }) {
        slot_taken: CollectionSlotGone
    }
    return Pickup.create {
        set: {
            shipment_id: shipment.id
            booking: booking.id
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/add_shipment.jadpo",
            r#"use shipping/entities { Shipment }

/// Adds a shipment.
action add_shipment(note: Shipment.note) -> Shipment {
    return Shipment.create {
        set: {
            note: note
            label: none
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/book_shipment.jadpo",
            r#"use app { couriers }
use shipping/entities { Shipment, ShipmentNotFound }
use shipping/failures { CollectionSlotGone, LabelInvalid }
use shipping/types { Label }
use shared/providers/acme_couriers

/// Reads a shipment, books its collection, then labels it with its note
/// and the booking: the read comes before the call, the write after it.
decline step.late_failure: "A shipment's note and booking always make a label"
action book_shipment(shipment_id: Shipment.id, postcode: AcmeCouriers.Postcode, slot: AcmeCouriers.SlotId) fails ShipmentNotFound, CollectionSlotGone, LabelInvalid -> Shipment {
    var shipment: Shipment = attempt Shipment.get(shipment_id)
    var booking: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: postcode
        slot_id: slot
    }) {
        slot_taken: CollectionSlotGone
    }
    var label: Label = attempt Label(shipment.note + " " + booking.id) {
        invalid: LabelInvalid
    }
    return Shipment.update(shipment) {
        set: {
            label: label
        }
    }
}
"#
            .to_string(),
        ),
    ]
}
