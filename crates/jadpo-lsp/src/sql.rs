//! A query's generated SQL in its hover (area 6; area 14, "Editor
//! support"): the statements `jadpo sql <query>` prints, read from the
//! same plan, one block for the query and one for each include, in the
//! order they are sent. Never written into the source. As `jadpo sql`
//! does, it waits for a project without errors, whose plan is built once
//! and kept until the next edit.

use std::sync::Arc;

use jadpo_graph::Key;
use jadpo_plan::Plan;
use jadpo_std::read::IncludeRead;

use crate::server::Server;

impl Server {
    /// The hover's SQL part for the query `key`.
    pub(crate) fn query_sql(&mut self, key: &Key) -> String {
        let Some(plan) = self.query_plan() else {
            return "SQL shown once the project has no errors, as `jadpo sql` shows it".to_string();
        };
        let Some(planned) = plan.queries().iter().find(|query| &query.key == key) else {
            return String::new();
        };
        let mut statements = vec![(
            format!("query {}", planned.name),
            planned.read.select.sql.clone(),
        )];
        let model = self.loaded().and_then(|loaded| {
            jadpo_check::data::queries::query_models(&loaded.db, loaded.project)
                .queries
                .iter()
                .find(|query| &query.key == key)
                .cloned()
        });
        let names = model
            .map(|model| include_names(&model.includes))
            .unwrap_or_default();
        let mut found = Vec::new();
        includes(&planned.read.select.includes, &mut found);
        for (i, sql) in found.into_iter().enumerate() {
            let name = names.get(i).map_or("include", String::as_str);
            statements.push((format!("include {name}"), sql));
        }
        let mut out = match statements.len() {
            1 => format!(
                "sends 1 statement, as `jadpo sql {}` shows it",
                planned.name
            ),
            n => format!(
                "sends {n} statements, one for the query and one for each include, as `jadpo sql {}` shows them",
                planned.name
            ),
        };
        for setting in &planned.read.settings {
            out.push_str(&format!("  \nfirst `{setting}`"));
        }
        for (heading, sql) in statements {
            out.push_str(&format!(
                "\n\n```sql\n-- {heading}\n{}\n```",
                sql.trim_end()
            ));
        }
        out
    }

    fn query_plan(&mut self) -> Option<Arc<Plan>> {
        if let Some(plan) = &self.plan {
            return Some(Arc::clone(plan));
        }
        let loaded = self.loaded()?;
        if !jadpo_check::check(&loaded.db, loaded.project).is_empty() {
            return None;
        }
        let plan = Arc::new(Plan::build(&loaded.db, loaded.project));
        self.plan = Some(Arc::clone(&plan));
        Some(plan)
    }
}

/// Each include's SQL, depth first, in the order `jadpo sql` prints them.
fn includes(reads: &[IncludeRead], out: &mut Vec<String>) {
    for read in reads {
        out.push(read.select.sql.clone());
        includes(&read.select.includes, out);
    }
}

/// Each include's name, in the same order.
fn include_names(model: &[jadpo_check::data::queries::Include]) -> Vec<String> {
    let mut out = Vec::new();
    for include in model {
        out.push(include.name.clone());
        out.extend(include_names(&include.includes));
    }
    out
}
