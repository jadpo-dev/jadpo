//! The addresses customers register for outgoing webhooks (area 16,
//! "Outgoing webhooks"; chunk C1b): the entity whose `webhooks:` lists
//! events as `jadpo-std`'s [`WebhooksPlan`], its generated operations'
//! statements limited by the entity's grants as a service's keys are, and
//! what each delivery reads.

use jadpo_check::policy::Effect;
use jadpo_db::{Db, Project};
use jadpo_sql::outgoing as statements;
use jadpo_std::outgoing::WebhooksPlan;

use crate::queries::Planner;

impl Planner<'_> {
    /// The plan of the entity whose records register addresses; `None`
    /// when none does.
    pub(crate) fn webhooks(&self, db: &dyn Db, project: Project) -> Option<WebhooksPlan> {
        let found = jadpo_check::outgoing::webhooks(db, project)
            .entity
            .as_ref()?;
        let entity = self.model.entity(&found.entity)?;
        let store = entity.store.clone()?;
        let table = entity.table.clone();
        let holder = statements::holder();
        // The record being registered on is `register_webhook`'s first
        // parameter.
        let new_holder = jadpo_sql::placeholder(0);
        Some(WebhooksPlan {
            entity: entity.name.clone(),
            store,
            lock: self.gated(entity, Effect::Update, 1, &new_holder, |held| {
                statements::lock(&table, held)
            }),
            lock_plain: statements::lock(&table, "TRUE"),
            list: self.gated(entity, Effect::Read, 1, &holder, statements::list),
            remove: self.gated(entity, Effect::Update, 1, &holder, statements::remove),
            addresses: statements::addresses(&table),
        })
    }
}
