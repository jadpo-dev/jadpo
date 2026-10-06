//! Reserved liveness routes must stay local even while dependencies are down.
use super::TargetGenerator;
use jadpo_syntax::{
    Expression, HttpMethod, LiteralKind, RecordKind, RouteDeclaration, RouteSuccess, Statement,
};

impl TargetGenerator<'_> {
    pub(super) fn authored_liveness_response(&self, route: &RouteDeclaration) -> Option<String> {
        if !route.public
            || route.fresh_authority
            || !route.path_fields.is_empty()
            || route.query.is_some()
            || !route.headers.is_empty()
            || route.input.is_some()
            || route.run.is_some()
            || route.deadline.is_some()
            || route.success != RouteSuccess::Ok
        {
            return None;
        }
        let output = route.output.as_ref()?;
        if output.nullable || !output.arguments.is_empty() || output.path.len() != 1 {
            return None;
        }
        let record = self.records.get(&output.path[0].text)?;
        if record.kind != RecordKind::Output {
            return None;
        }
        let action = route.inline_action.as_ref()?;
        if !action.failures.is_empty() {
            return None;
        }
        let [Statement::Return(returned)] = action.body.statements.as_slice() else {
            return None;
        };
        let Expression::Construction(value) = &returned.value else {
            return None;
        };
        if value.target.path.len() != 1
            || value.target.path[0].text != record.name.text
            || !value
                .fields
                .iter()
                .all(|field| self.liveness_constant(&field.value))
        {
            return None;
        }
        Some(self.validation_expression(
            output,
            &self.expression(&returned.value),
            "\"response.body\"",
        ))
    }

    fn liveness_constant(&self, expression: &Expression) -> bool {
        match expression {
            Expression::Literal(value) => matches!(
                value.kind,
                LiteralKind::String
                    | LiteralKind::Integer
                    | LiteralKind::Decimal
                    | LiteralKind::Boolean
                    | LiteralKind::None
            ),
            Expression::Name(name) if name.path.len() == 2 => self
                .enums
                .get(&name.path[0].text)
                .is_some_and(|enumeration| {
                    enumeration.variants.iter().any(|variant| {
                        variant.name.text == name.path[1].text && variant.fields.is_empty()
                    })
                }),
            _ => false,
        }
    }

    pub(super) fn is_liveness_route(route: &RouteDeclaration) -> bool {
        route.method == HttpMethod::Get && route.path == "/health/live"
    }
}

#[cfg(test)]
mod tests {
    use crate::{analyze_sources, target::derive_target};
    use jadpo_syntax::SourceFile;
    use std::path::Path;

    const HEALTH: &str = "enum HealthStatus { healthy }\noutput PublicHealth { status: HealthStatus }\nroute GET /health/live { auth: none output: PublicHealth action: { return PublicHealth { status: HealthStatus.healthy } } }\n";

    fn analyze(source: &str) -> crate::AnalyzedProject {
        let project = analyze_sources(vec![SourceFile::new(
            Path::new("liveness.jadpo").to_path_buf(),
            source.to_owned(),
        )])
        .expect("source should analyze");
        assert!(
            project.syntax.diagnostics().next().is_none(),
            "{:?}",
            project.syntax.diagnostics().collect::<Vec<_>>()
        );
        assert!(
            project.semantics.diagnostics.is_empty(),
            "{:?}",
            project.semantics.diagnostics
        );
        assert!(
            project.typing.diagnostics.is_empty(),
            "{:?}",
            project.typing.diagnostics
        );
        assert!(
            project.failures.diagnostics.is_empty(),
            "{:?}",
            project.failures.diagnostics
        );
        project
    }

    #[test]
    fn authored_liveness_preserves_constant_output_and_ordinary_fallback() {
        let project = analyze(HEALTH);
        let artifacts = derive_target(Path::new("."), &project).expect("local output supported");
        let app = &artifacts[0].contents;
        assert!(app.contains("return json(200, validate_PublicHealth({ status: \"healthy\" }, \"response.body\"), requestId)"));
        assert!(!app.contains("matchRoutePath(\"/health/live\""));
        let project = analyze(&HEALTH.replace("/health/live", "/health"));
        let artifacts = derive_target(Path::new("."), &project).expect("ordinary authored route");
        assert!(artifacts[0]
            .contents
            .contains("return json(200, { status: \"live\" }, requestId)"));
    }

    #[test]
    fn authored_liveness_rejects_protected_calls_and_runtime_reads() {
        let candidates = [
            HEALTH.replace("auth: none", ""),
            format!(
                "function status() -> HealthStatus {{ return HealthStatus.healthy }}\n{}",
                HEALTH.replace("status: HealthStatus.healthy", "status: status()")
            ),
            HEALTH.replace(
                "return PublicHealth { status: HealthStatus.healthy }",
                "var status = HealthStatus.healthy\nreturn PublicHealth { status: status }",
            ),
            HEALTH
                .replace(
                    "output PublicHealth { status: HealthStatus }",
                    "output PublicHealth { status: HealthStatus\nat: Instant }",
                )
                .replace(
                    "status: HealthStatus.healthy }",
                    "status: HealthStatus.healthy\nat: clock.now }",
                ),
        ];
        for source in candidates {
            let project = analyze(&source);
            let diagnostic = derive_target(Path::new("."), &project)
                .expect_err("runtime-dependent health must reject");
            assert_eq!(diagnostic.code, "JADPO_TARGET_LIVENESS_NOT_LOCAL");
            let span = diagnostic
                .primary
                .expect("diagnostic should locate authored route");
            assert_eq!(span.source, "liveness.jadpo");
            assert!(source[span.start..span.end].starts_with("route GET /health/live"));
        }
    }
}
