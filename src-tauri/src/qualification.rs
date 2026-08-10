use std::collections::HashSet;

use crate::models::{
    CitedReason, FitAssessment, FitFeature, HardGate, ProspectPackage, SalesPlay, SearchBrief,
};

#[derive(Debug, Clone)]
pub struct SearchQualification {
    pub assessment: FitAssessment,
    pub features: Vec<FitFeature>,
    pub state: String,
    pub evidence_confidence: String,
    pub rejection_reasons: Vec<String>,
}

pub fn assess_for_search(package: &ProspectPackage, brief: &SearchBrief) -> SearchQualification {
    let corpus = package
        .evidence
        .iter()
        .map(|item| format!("{} {}", item.title, item.excerpt))
        .chain(std::iter::once(format!(
            "{} {} {}",
            package.account.name,
            package.account.location.as_deref().unwrap_or_default(),
            package.account.industry.as_deref().unwrap_or_default()
        )))
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    let identity_ids = package
        .evidence
        .iter()
        .filter(|item| {
            item.source_kind.contains("record")
                || item.source_kind == "company_page"
                || item.source_kind == "user_import"
        })
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let identity_pass = package.account.domain.is_some() || !identity_ids.is_empty();
    let geography_pass = geography_matches(package, &brief.geography, &corpus);
    let excluded = brief
        .exclusions
        .iter()
        .find(|value| !value.trim().is_empty() && corpus.contains(&value.to_ascii_lowercase()));
    let hard_gates = vec![
        HardGate {
            label: "Stable public identity".into(),
            status: if identity_pass { "pass" } else { "unknown" }.into(),
            reason: if identity_pass {
                "A company domain or attributable public business record anchors this identity."
                    .into()
            } else {
                "No stable company domain or authoritative business record was found.".into()
            },
        },
        HardGate {
            label: format!("Geography: {}", brief.geography),
            status: if geography_pass { "pass" } else { "unknown" }.into(),
            reason: if geography_pass {
                "The account location matches the approved search geography.".into()
            } else {
                "The approved geography is not explicit in current public evidence.".into()
            },
        },
        HardGate {
            label: "Approved exclusions".into(),
            status: if excluded.is_some() { "fail" } else { "pass" }.into(),
            reason: excluded
                .map(|value| format!("Public evidence matched the exclusion: {value}."))
                .unwrap_or_else(|| "No approved exclusion was found in current evidence.".into()),
        },
    ];

    let mut features = Vec::new();
    for segment in &brief.segments {
        let needles = segment
            .label
            .split(|character: char| !character.is_ascii_alphanumeric())
            .filter(|part| part.len() >= 4)
            .map(str::to_ascii_lowercase)
            .chain(
                segment
                    .discovery_category
                    .iter()
                    .map(|value| value.replace('_', " ")),
            )
            .collect::<Vec<_>>();
        let evidence_ids = matching_evidence_ids(package, &needles);
        if !evidence_ids.is_empty() {
            features.push(FitFeature {
                rule_id: format!("segment:{}", segment.id),
                label: segment.label.clone(),
                component: "icp".into(),
                value: "present".into(),
                points_awarded: 70,
                evidence_ids,
                confidence: "high".into(),
            });
        }
    }
    if geography_pass {
        features.push(FitFeature {
            rule_id: "geography".into(),
            label: format!("Located in {}", brief.geography),
            component: "icp".into(),
            value: "present".into(),
            points_awarded: 20,
            evidence_ids: identity_ids.clone(),
            confidence: "medium".into(),
        });
    }
    let mut awarded_signal_evidence = HashSet::new();
    for rule in &brief.positive_signals {
        let evidence_ids = matching_signal_evidence_ids(package, &rule.keywords)
            .into_iter()
            .filter(|id| awarded_signal_evidence.insert(id.clone()))
            .collect::<Vec<_>>();
        if !evidence_ids.is_empty() {
            features.push(FitFeature {
                rule_id: rule.id.clone(),
                label: rule.label.clone(),
                component: rule.component.clone(),
                value: "present".into(),
                points_awarded: rule.points.clamp(0, 100),
                evidence_ids,
                confidence: "high".into(),
            });
        }
    }

    let icp_fit = features
        .iter()
        .filter(|item| item.component == "icp")
        .map(|item| item.points_awarded)
        .sum::<i64>()
        .clamp(0, 100);
    let signal_relevance = features
        .iter()
        .filter(|item| item.component == "signal")
        .map(|item| item.points_awarded)
        .sum::<i64>()
        .clamp(0, 100);
    let persona_fit = persona_score(package, &brief.target_personas);
    let evidence_contactability = evidence_score(package, identity_pass);
    let failed = hard_gates.iter().any(|gate| gate.status == "fail");
    let unknown = hard_gates.iter().any(|gate| gate.status == "unknown");
    let total = if failed {
        0
    } else {
        weighted(
            icp_fit,
            signal_relevance,
            persona_fit,
            evidence_contactability,
        )
    };
    let direct_signal = features
        .iter()
        .any(|feature| feature.component == "signal" && !feature.evidence_ids.is_empty());
    let evidence_confidence = evidence_confidence(package, identity_pass);
    let mut rejection_reasons = Vec::new();
    if failed {
        rejection_reasons.extend(
            hard_gates
                .iter()
                .filter(|gate| gate.status == "fail")
                .map(|gate| gate.reason.clone()),
        );
    }
    if !direct_signal {
        rejection_reasons
            .push("No direct public evidence matched an approved product-relevant signal.".into());
    }
    if total < brief.qualification_threshold {
        rejection_reasons.push(format!(
            "Deterministic fit score {total} is below the approved threshold {}.",
            brief.qualification_threshold
        ));
    }
    let state = if failed || (!direct_signal && total < brief.qualification_threshold - 20) {
        "rejected"
    } else if !unknown && direct_signal && total >= brief.qualification_threshold {
        "qualified"
    } else {
        "needs_review"
    }
    .to_string();
    let reasons = features
        .iter()
        .filter(|feature| !feature.evidence_ids.is_empty())
        .map(|feature| CitedReason {
            text: feature.label.clone(),
            evidence_ids: feature.evidence_ids.clone(),
        })
        .collect();
    SearchQualification {
        assessment: FitAssessment {
            hard_gates,
            icp_fit,
            signal_relevance,
            persona_fit,
            evidence_contactability,
            total,
            reasons,
        },
        features,
        state,
        evidence_confidence,
        rejection_reasons,
    }
}

pub fn assess_for_play(package: &ProspectPackage, play: &SalesPlay) -> FitAssessment {
    let corpus = package
        .evidence
        .iter()
        .map(|item| format!("{} {}", item.title, item.excerpt))
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let industry_match = play.icp_industries.iter().any(|industry| {
        !industry.trim().is_empty() && corpus.contains(&industry.to_ascii_lowercase())
    });
    let excluded = play.exclusions.iter().find(|exclusion| {
        !exclusion.trim().is_empty() && corpus.contains(&exclusion.to_ascii_lowercase())
    });
    let hard_gates = vec![HardGate {
        label: "Approved exclusions".into(),
        status: if excluded.is_some() { "fail" } else { "pass" }.into(),
        reason: excluded
            .map(|value| format!("Evidence matched the exclusion: {value}."))
            .unwrap_or_else(|| "No approved exclusion was found in current evidence.".into()),
    }];
    let icp_fit = if play.icp_industries.is_empty() {
        50
    } else if industry_match {
        85
    } else {
        25
    };
    let signal_relevance = (package.signals.len() as i64 * 25).clamp(0, 100);
    let persona_fit = persona_score(package, &play.target_personas);
    let evidence_contactability = evidence_score(package, package.account.domain.is_some());
    let failed = hard_gates.iter().any(|gate| gate.status == "fail");
    FitAssessment {
        hard_gates,
        icp_fit,
        signal_relevance,
        persona_fit,
        evidence_contactability,
        total: if failed {
            0
        } else {
            weighted(
                icp_fit,
                signal_relevance,
                persona_fit,
                evidence_contactability,
            )
        },
        reasons: package
            .evidence
            .iter()
            .take(3)
            .map(|item| CitedReason {
                text: format!("Public evidence: {}", item.title),
                evidence_ids: vec![item.id.clone()],
            })
            .collect(),
    }
}

fn matching_evidence_ids(package: &ProspectPackage, keywords: &[String]) -> Vec<String> {
    let needles = keywords
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| value.len() >= 3)
        .collect::<Vec<_>>();
    package
        .evidence
        .iter()
        .filter(|item| {
            let text = normalized_search_text(&format!("{} {}", item.title, item.excerpt));
            needles.iter().any(|needle| contains_phrase(&text, needle))
        })
        .map(|item| item.id.clone())
        .collect()
}

fn matching_signal_evidence_ids(package: &ProspectPackage, keywords: &[String]) -> Vec<String> {
    let needles = keywords
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| value.len() >= 3)
        .collect::<Vec<_>>();
    package
        .evidence
        .iter()
        .filter(|item| {
            let trusted_kind =
                !item.source_kind.ends_with("_record") && item.source_kind != "user_import";
            let usable = item.freshness != "stale" && item.quality_flags.is_empty();
            let text = normalized_search_text(&format!("{} {}", item.title, item.excerpt));
            trusted_kind && usable && needles.iter().any(|needle| contains_phrase(&text, needle))
        })
        .map(|item| item.id.clone())
        .collect()
}

fn normalized_search_text(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_phrase(normalized_text: &str, needle: &str) -> bool {
    let normalized_needle = normalized_search_text(needle);
    !normalized_needle.is_empty()
        && format!(" {normalized_text} ").contains(&format!(" {normalized_needle} "))
}

fn geography_matches(package: &ProspectPackage, geography: &str, corpus: &str) -> bool {
    let requested = geography.to_ascii_lowercase();
    package.account.location.as_ref().is_some_and(|location| {
        let location = location.to_ascii_lowercase();
        requested.contains(&location)
            || location.contains(&requested)
            || requested.contains("united states")
    }) || corpus.contains(&requested)
        || (requested.contains("united states")
            && [" united states", " u.s.", " usa "]
                .iter()
                .any(|value| corpus.contains(value)))
}

fn persona_score(package: &ProspectPackage, personas: &[String]) -> i64 {
    if personas.is_empty() {
        return 0;
    }
    let personas = personas
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if package.people.iter().any(|person| {
        person.title.as_ref().is_some_and(|title| {
            let title = title.to_ascii_lowercase();
            personas
                .iter()
                .any(|persona| title.contains(persona) || persona.contains(&title))
        })
    }) {
        100
    } else if package.evidence.iter().any(|item| {
        personas
            .iter()
            .any(|persona| item.excerpt.to_ascii_lowercase().contains(persona))
    }) {
        65
    } else {
        0
    }
}

fn evidence_score(package: &ProspectPackage, identity_pass: bool) -> i64 {
    let identity = if identity_pass { 35 } else { 0 };
    let evidence = (package.evidence.len() as i64 * 12).clamp(0, 25);
    let contact = if package
        .contacts
        .iter()
        .any(|item| item.kind.contains("email"))
    {
        40
    } else if package
        .contacts
        .iter()
        .any(|item| item.kind == "company_main")
    {
        25
    } else {
        0
    };
    (identity + evidence + contact).clamp(0, 100)
}

fn evidence_confidence(package: &ProspectPackage, identity_pass: bool) -> String {
    let publishers = package
        .evidence
        .iter()
        .filter_map(|item| url::Url::parse(&item.source_url).ok())
        .filter_map(|url| url.host_str().map(str::to_owned))
        .collect::<HashSet<_>>();
    if identity_pass
        && package.evidence.len() >= 2
        && publishers.len() >= 2
        && package
            .evidence
            .iter()
            .all(|item| item.quality_flags.is_empty())
    {
        "high"
    } else if identity_pass && !package.evidence.is_empty() {
        "medium"
    } else {
        "low"
    }
    .into()
}

fn weighted(icp: i64, signal: i64, persona: i64, evidence: i64) -> i64 {
    ((icp.clamp(0, 100) as f64 * 0.35)
        + (signal.clamp(0, 100) as f64 * 0.30)
        + (persona.clamp(0, 100) as f64 * 0.20)
        + (evidence.clamp(0, 100) as f64 * 0.15))
        .round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Account, ContactPoint, Evidence, ProspectPackage, TargetSegment};

    fn package() -> ProspectPackage {
        ProspectPackage {
            account: Account {
                id: "a".into(),
                name: "Acme Plumbing".into(),
                domain: Some("acme.example".into()),
                website_url: Some("https://acme.example".into()),
                industry: Some("Plumbing".into()),
                employee_band: None,
                location: Some("United States".into()),
                status: "researched".into(),
                score: None,
                score_label: None,
                researched_at: None,
                created_at: "now".into(),
                saved_at: Some("now".into()),
                favorited_at: None,
                trashed_at: None,
            },
            people: vec![],
            contacts: vec![ContactPoint {
                id: "c".into(),
                account_id: "a".into(),
                person_id: None,
                kind: "company_main".into(),
                value: "+12125550100".into(),
                evidence_id: "e".into(),
                confidence: "medium".into(),
            }],
            evidence: vec![Evidence {
                id: "e".into(),
                account_id: "a".into(),
                source_url: "https://acme.example".into(),
                source_kind: "company_page".into(),
                title: "Emergency plumbing".into(),
                excerpt: "Acme provides 24 hour emergency plumbing. Call now for service.".into(),
                retrieved_at: "now".into(),
                published_at: None,
                content_hash: "h".into(),
                confidence: "high".into(),
                freshness: "fresh".into(),
                quality_flags: vec![],
            }],
            signals: vec![],
            assessment: None,
            draft: None,
            call_brief: None,
            suppressed: false,
            latest_outcome: None,
            discovery_context: None,
        }
    }

    #[test]
    fn scoring_is_deterministic_and_requires_direct_signal() {
        let brief = SearchBrief {
            id: "b".into(),
            query: "voice intake for plumbers".into(),
            product_summary: "Voice intake".into(),
            customer_outcome: "Capture calls".into(),
            segments: vec![TargetSegment {
                id: "plumbing".into(),
                label: "Plumbing".into(),
                discovery_category: Some("plumber".into()),
            }],
            geography: "United States".into(),
            company_sizes: vec![],
            target_personas: vec!["Owner".into()],
            positive_signals: vec![crate::models::QualificationRule {
                id: "after_hours".into(),
                label: "After-hours service".into(),
                component: "signal".into(),
                keywords: vec!["24 hour".into(), "emergency".into()],
                points: 100,
                hard_gate: false,
            }],
            exclusions: vec![],
            discovery_lanes: vec![],
            result_limit: 25,
            qualification_threshold: 45,
            quality_band: 4,
            approved: true,
            assumptions: vec![],
            interpreter: "manual_rules".into(),
            model_tag: None,
            model_digest: None,
            prompt_version: crate::models::SEARCH_PROMPT_VERSION.into(),
            created_at: "now".into(),
        };
        let result = assess_for_search(&package(), &brief);
        assert_eq!(result.state, "qualified");
        assert!(result.assessment.total >= 45);
        assert!(result
            .features
            .iter()
            .any(|item| item.rule_id == "after_hours"));
    }
}
