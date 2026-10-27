use std::collections::HashSet;

use crate::catalog;
use crate::matcher::{self, Text};
use crate::models::{
    CitedReason, FitAssessment, FitFeature, HardGate, ProspectPackage, SalesPlay, SearchBrief,
    TargetSegment,
};
use crate::regions;

/// Discovery records whose source query itself proved the organization type:
/// an OpenStreetMap element returned by a category filter, or a Wikidata item
/// returned as an instance of a resolved class.
const TYPE_SCOPED_RECORDS: &[&str] = &["openstreetmap_record", "wikidata_record"];

#[derive(Debug, Clone)]
pub struct SearchQualification {
    pub assessment: FitAssessment,
    pub features: Vec<FitFeature>,
    pub state: String,
    pub evidence_confidence: String,
    pub rejection_reasons: Vec<String>,
}

pub fn assess_for_search(package: &ProspectPackage, brief: &SearchBrief) -> SearchQualification {
    let texts = evidence_texts(package);
    let corpus = Text::new(
        &package
            .evidence
            .iter()
            .map(|item| {
                if has_synthetic_title(&item.source_kind) {
                    item.excerpt.clone()
                } else {
                    format!("{} {}", item.title, item.excerpt)
                }
            })
            .chain(std::iter::once(format!(
                "{} {} {}",
                package.account.name,
                package.account.location.as_deref().unwrap_or_default(),
                package.account.industry.as_deref().unwrap_or_default()
            )))
            .collect::<Vec<_>>()
            .join(" \n "),
    );

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
        .find(|value| !value.trim().is_empty() && corpus.mentions(value));
    let mut hard_gates = vec![
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
        let (label, evidence_ids) = if segment.id == catalog::GENERAL_BUSINESS {
            // "General private businesses" names no type a page could state.
            // Only a discovery source that searched by type can establish it.
            (
                "Organization type established by the discovery source".to_string(),
                package
                    .evidence
                    .iter()
                    .filter(|item| TYPE_SCOPED_RECORDS.contains(&item.source_kind.as_str()))
                    .map(|item| item.id.clone())
                    .collect::<Vec<_>>(),
            )
        } else {
            let needles = segment_needles(segment);
            (
                segment.label.clone(),
                package
                    .evidence
                    .iter()
                    .zip(&texts)
                    .filter(|(_, text)| needles.iter().any(|needle| text.mentions(needle)))
                    .map(|(item, _)| item.id.clone())
                    .collect::<Vec<_>>(),
            )
        };
        if !evidence_ids.is_empty() {
            features.push(FitFeature {
                rule_id: format!("segment:{}", segment.id),
                label,
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
    // One sentence is one piece of evidence. Keyed on the excerpt text rather
    // than the row id, because the same line can be stored under two rows.
    let mut awarded_excerpts = HashSet::new();
    for rule in &brief.positive_signals {
        let matched = signal_matches(package, &texts, &rule.keywords);
        if matched.is_empty() && rule.hard_gate {
            hard_gates.push(HardGate {
                label: format!("Required signal: {}", rule.label),
                // Absent evidence is not counter-evidence, so this is unknown
                // rather than a failure: the candidate stays reviewable.
                status: "unknown".into(),
                reason: format!(
                    "Required signal not found in public evidence: {}.",
                    rule.label
                ),
            });
        }
        // The rule consumes one sentence, preferring the line the crawler
        // extracted for this very rule. Consuming every matching sentence
        // starved later rules: a line that mentions "literary department"
        // and "submissions" must still be available to the submissions rule.
        let mut available = matched
            .into_iter()
            .filter(|item| !awarded_excerpts.contains(&matcher::normalized(&item.excerpt)))
            .collect::<Vec<_>>();
        available
            .sort_by_key(|item| !(item.source_kind == "rule_excerpt" && item.title == rule.label));
        if let Some(first) = available.first() {
            awarded_excerpts.insert(matcher::normalized(&first.excerpt));
        }
        let evidence_ids = available
            .into_iter()
            .map(|item| item.id.clone())
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
    rejection_reasons.extend(
        hard_gates
            .iter()
            .filter(|gate| gate.status == "unknown" && gate.label.starts_with("Required signal"))
            .map(|gate| gate.reason.clone()),
    );
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
    let corpus = Text::new(
        &package
            .evidence
            .iter()
            .map(|item| format!("{} {}", item.title, item.excerpt))
            .collect::<Vec<_>>()
            .join(" \n "),
    );
    let industry_match = play
        .icp_industries
        .iter()
        .any(|industry| !industry.trim().is_empty() && corpus.mentions(industry));
    let excluded = play
        .exclusions
        .iter()
        .find(|exclusion| !exclusion.trim().is_empty() && corpus.mentions(exclusion));
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

/// Evidence rows that quote a whole crawled page's opening text.
const PAGE_LEVEL_KINDS: &[&str] = &[
    "company_page",
    "contact_page",
    "careers_page",
    "company_news",
    "leadership_page",
];

/// Rows that quote one line chosen for a rule.
fn is_line_level(source_kind: &str) -> bool {
    source_kind == "rule_excerpt" || source_kind.starts_with("workflow_")
}

/// Evidence kinds whose title the application wrote rather than the page.
///
/// Matching those titles let a fact satisfy its own rule: an excerpt titled
/// "After-hours service evidence" matched "after hours" even when the quoted
/// line said "we do not offer emergency service".
fn has_synthetic_title(source_kind: &str) -> bool {
    ["workflow_", "rule_", "persona_", "technology_"]
        .iter()
        .any(|prefix| source_kind.starts_with(prefix))
}

/// The published text of each evidence row, in order.
fn evidence_texts(package: &ProspectPackage) -> Vec<Text> {
    package
        .evidence
        .iter()
        .map(|item| {
            if has_synthetic_title(&item.source_kind) {
                Text::new(&item.excerpt)
            } else {
                Text::new(&format!("{} {}", item.title, item.excerpt))
            }
        })
        .collect()
}

/// Phrases that establish a segment: its catalog id, curated synonyms, and its
/// full label as one phrase. Never single words split out of the label, which
/// is how "Law firms" used to match any page saying "firms".
fn segment_needles(segment: &TargetSegment) -> Vec<String> {
    let mut needles = vec![segment.label.clone()];
    if let Some(category) = segment
        .discovery_category
        .as_deref()
        .and_then(catalog::find)
        .or_else(|| catalog::find(&segment.id))
    {
        needles.push(category.id.replace('_', " "));
        needles.extend(category.synonyms.iter().map(|value| value.to_string()));
    }
    needles
}

/// Evidence that affirms any of `keywords`, from sources trusted for signals.
fn signal_matches<'a>(
    package: &'a ProspectPackage,
    texts: &[Text],
    keywords: &[String],
) -> Vec<&'a crate::models::Evidence> {
    let needles = keywords
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| value.len() >= 3)
        .collect::<Vec<_>>();
    // A page yields one row for the page and one per line that supports a
    // rule. Crediting both would let one sentence count twice, once inside the
    // page excerpt and once on its own, so a page row is only eligible for a
    // page that produced no line-level evidence (older research, or a page
    // whose title alone carries the signal).
    let line_level_pages = package
        .evidence
        .iter()
        .filter(|item| is_line_level(&item.source_kind))
        .map(|item| item.source_url.as_str())
        .collect::<HashSet<_>>();
    package
        .evidence
        .iter()
        .zip(texts)
        .filter(|(item, text)| {
            let trusted_kind = !item.source_kind.ends_with("_record")
                && item.source_kind != "user_import"
                && item.source_kind != "persona_excerpt"
                && (!PAGE_LEVEL_KINDS.contains(&item.source_kind.as_str())
                    || !line_level_pages.contains(item.source_url.as_str()));
            // Only a contradiction disqualifies evidence from scoring. The
            // `incomplete` flag fires on any excerpt under 48 characters, which
            // silently discarded short but perfectly valid workflow evidence
            // such as "24/7 emergency service".
            let usable = item.freshness != "stale"
                && !item
                    .quality_flags
                    .iter()
                    .any(|flag| flag == "contradictory");
            trusted_kind && usable && needles.iter().any(|needle| matches_keyword(text, needle))
        })
        .map(|(item, _)| item)
        .collect()
}

#[cfg(test)]
fn matching_signal_evidence_ids(package: &ProspectPackage, keywords: &[String]) -> Vec<String> {
    signal_matches(package, &evidence_texts(package), keywords)
        .into_iter()
        .map(|item| item.id.clone())
        .collect()
}

/// Keyword variants that mean the same thing in ordinary business copy.
///
/// The matcher already handles plurals, verb forms and a few words in between;
/// this table covers genuine synonyms. A site saying "open 24 hours a day" or
/// "call today" otherwise matches nothing, scores zero signal relevance and
/// cannot qualify.
fn keyword_variants(needle: &str) -> Vec<String> {
    let mut variants = vec![needle.to_string()];
    let extra: &[&str] = match needle {
        "24 7" | "24/7" | "24 hour" => &[
            "24 hours",
            "24 hour",
            "24 7",
            "around the clock",
            "open 24 hours",
            "always open",
        ],
        "after hours" => &["after hours", "evening service", "nights and weekends"],
        "emergency service" => &[
            "emergency service",
            "emergency services",
            "emergency repair",
            "emergency call",
            "same day service",
        ],
        "call now" | "call us" => &[
            "call now",
            "call us",
            "call today",
            "give us a call",
            "call our office",
        ],
        "phone consultation" | "speak with" => &[
            "phone consultation",
            "speak with",
            "talk to a",
            "free consultation",
        ],
        "schedule an appointment" | "appointment" => &[
            "schedule an appointment",
            "book an appointment",
            "request an appointment",
            "make an appointment",
            "appointments",
        ],
        "request service" => &["request service", "service request", "request a quote"],
        "dispatcher" => &["dispatcher", "dispatchers", "dispatch"],
        "receptionist" => &["receptionist", "receptionists", "front desk"],
        "intake specialist" => &["intake specialist", "intake coordinator", "client intake"],
        "customer service representative" => &[
            "customer service representative",
            "customer service rep",
            "call center",
        ],
        "we are hiring" | "hiring" => &["we are hiring", "we re hiring", "now hiring"],
        "open positions" | "job openings" => {
            &["open positions", "job openings", "current openings"]
        }
        // Quoted: "location" in the singular is evidence of one premise, so
        // these must not be stemmed into each other.
        "locations" | "\"locations\"" | "service areas" | "areas we serve" => &[
            "\"locations\"",
            "service areas",
            "areas we serve",
            "\"multiple locations\"",
        ],
        "new location" => &["new location", "newly opened", "now open in"],
        "launched" | "introducing" => &["launched", "introducing", "we launched", "announcing"],
        _ => &[],
    };
    variants.extend(extra.iter().map(|value| value.to_string()));
    variants.sort();
    variants.dedup();
    variants
}

pub(crate) fn matches_keyword(text: &Text, needle: &str) -> bool {
    keyword_variants(needle)
        .iter()
        .any(|variant| text.affirms(variant))
}

/// Whether the account is inside the approved geography.
///
/// A recorded location is the strongest provenance, so when it can be placed it
/// decides: a dentist recorded in New Jersey is not in Pennsylvania however
/// often its pages mention Philadelphia. Only an account with no placeable
/// location falls back to the evidence naming the place.
fn geography_matches(package: &ProspectPackage, geography: &str, corpus: &Text) -> bool {
    let requested = geography.trim();
    let location = package.account.location.as_deref();
    let Some(wanted) = regions::place(requested) else {
        let normalized = matcher::normalized(requested);
        let nationwide = normalized.is_empty()
            || ["united states", "usa", "us", "u s", "nationwide", "america"]
                .iter()
                .any(|value| Text::new(&normalized).mentions(value));
        return if nationwide {
            location.is_some()
                || ["united states", "usa", "u s"]
                    .iter()
                    .any(|value| corpus.mentions(value))
        } else {
            location.is_some_and(|value| Text::new(value).mentions(requested))
                || corpus.mentions(requested)
        };
    };
    if let Some(found) = location.and_then(regions::place) {
        return match wanted.metro {
            Some(metro) => found.metro == Some(metro),
            None => found.state == wanted.state,
        };
    }
    match wanted.metro {
        Some(metro) => corpus.mentions(metro.trim_end_matches(" metro")),
        None => corpus.mentions(wanted.state),
    }
}

fn persona_score(package: &ProspectPackage, personas: &[String]) -> i64 {
    let personas = personas
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if personas.is_empty() {
        return 0;
    }
    if package.people.iter().any(|person| {
        person
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .is_some_and(|title| {
                let title = Text::new(title);
                personas.iter().any(|persona| title.mentions(persona))
            })
    }) {
        100
    } else if package
        .evidence
        .iter()
        // Only pages that introduce people. Any excerpt used to count, so a
        // company named "... Partners" matched the persona "Partner".
        .filter(|item| {
            matches!(
                item.source_kind.as_str(),
                "persona_excerpt" | "leadership_page"
            )
        })
        .any(|item| {
            let excerpt = Text::new(&item.excerpt);
            personas.iter().any(|persona| excerpt.mentions(persona))
        })
    {
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
                origin: "pack".into(),
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

    #[test]
    fn bounded_region_provenance_lets_an_untagged_mapped_business_qualify() {
        // The yield bug this guards: an OpenStreetMap element with no addr:*
        // tags used to arrive with location = None. The geography hard gate
        // then resolved "unknown", `state` could never become "qualified", and
        // finalize_search_ranking could never promote it to a strong match,
        // however good the company was. Discovery now records the bounded
        // region the element was found in, which is a fact the query proved.
        let mut package = package();
        package.account.location = Some("Philadelphia metro, Pennsylvania, United States".into());
        package.account.industry = None;
        package.evidence.insert(
            0,
            Evidence {
                id: "osm".into(),
                account_id: "a".into(),
                source_url: "https://www.openstreetmap.org/node/1".into(),
                source_kind: "openstreetmap_record".into(),
                title: "OpenStreetMap discovery record".into(),
                excerpt: "Acme Plumbing: Mapped plumber in the bounded Philadelphia metro sample; location established by the bounded search area, not a published address.".into(),
                retrieved_at: "now".into(),
                published_at: None,
                content_hash: "h2".into(),
                confidence: "medium".into(),
                freshness: "fresh".into(),
                quality_flags: vec![],
            },
        );

        let brief = SearchBrief {
            id: "b".into(),
            query: "voice intake for plumbers".into(),
            product_summary: "Voice intake".into(),
            customer_outcome: "Capture calls".into(),
            segments: vec![TargetSegment {
                id: "plumber".into(),
                label: "Plumbing services".into(),
                discovery_category: Some("plumber".into()),
            }],
            geography: "Pennsylvania".into(),
            company_sizes: vec![],
            target_personas: vec!["Owner".into()],
            positive_signals: vec![crate::models::QualificationRule {
                id: "after_hours".into(),
                label: "After-hours service".into(),
                component: "signal".into(),
                keywords: vec!["24 hour".into(), "emergency".into()],
                points: 100,
                hard_gate: false,
                origin: "pack".into(),
            }],
            exclusions: vec![],
            discovery_lanes: vec![],
            result_limit: 25,
            qualification_threshold: 45,
            quality_band: 12,
            approved: true,
            assumptions: vec![],
            interpreter: "manual_rules".into(),
            model_tag: None,
            model_digest: None,
            prompt_version: crate::models::SEARCH_PROMPT_VERSION.into(),
            created_at: "now".into(),
        };

        let result = assess_for_search(&package, &brief);
        let geography_gate = result
            .assessment
            .hard_gates
            .iter()
            .find(|gate| gate.label.starts_with("Geography"))
            .expect("geography gate must exist");
        assert_eq!(
            geography_gate.status, "pass",
            "bounded-region provenance must satisfy the geography gate"
        );
        assert_eq!(result.state, "qualified");
    }

    #[test]
    fn signal_matching_accepts_ordinary_business_phrasing() {
        let text = Text::new(
            "We are open 24 hours a day. Call today to book an appointment with our dispatch team.",
        );
        assert!(matches_keyword(&text, "24 hour"));
        assert!(matches_keyword(&text, "call now"));
        assert!(matches_keyword(&text, "schedule an appointment"));
        assert!(matches_keyword(&text, "dispatcher"));
        // Widening variants must not turn matching into a free-for-all.
        assert!(!matches_keyword(&text, "receptionist"));
        assert!(!matches_keyword(&text, "job openings"));
    }

    #[test]
    fn short_evidence_still_counts_toward_signal_scoring() {
        // The `incomplete` flag fires on any excerpt under 48 characters, and
        // used to disqualify the evidence from signal matching entirely.
        let mut package = package();
        package.evidence[0].excerpt = "24/7 emergency service".into();
        package.evidence[0].quality_flags = vec!["incomplete".into()];
        let ids =
            matching_signal_evidence_ids(&package, &["emergency service".into(), "24 hour".into()]);
        assert_eq!(ids, vec!["e".to_string()]);

        // A contradiction still disqualifies it.
        package.evidence[0].quality_flags = vec!["contradictory".into()];
        assert!(matching_signal_evidence_ids(
            &package,
            &["emergency service".into(), "24 hour".into()]
        )
        .is_empty());
    }

    #[test]
    fn a_missing_location_still_leaves_geography_unknown() {
        // The gate is only satisfied by real provenance. A candidate we cannot
        // place must still not claim to be in the approved geography.
        let mut package = package();
        package.account.location = None;
        package.account.industry = None;
        package.evidence[0].excerpt =
            "Acme provides 24 hour emergency plumbing service to local customers.".into();

        let brief = SearchBrief {
            id: "b".into(),
            query: "voice intake for plumbers".into(),
            product_summary: "Voice intake".into(),
            customer_outcome: "Capture calls".into(),
            segments: vec![],
            geography: "Pennsylvania".into(),
            company_sizes: vec![],
            target_personas: vec![],
            positive_signals: vec![],
            exclusions: vec![],
            discovery_lanes: vec![],
            result_limit: 25,
            qualification_threshold: 45,
            quality_band: 12,
            approved: true,
            assumptions: vec![],
            interpreter: "manual_rules".into(),
            model_tag: None,
            model_digest: None,
            prompt_version: crate::models::SEARCH_PROMPT_VERSION.into(),
            created_at: "now".into(),
        };
        let result = assess_for_search(&package, &brief);
        assert_eq!(
            result
                .assessment
                .hard_gates
                .iter()
                .find(|gate| gate.label.starts_with("Geography"))
                .unwrap()
                .status,
            "unknown"
        );
        assert_ne!(result.state, "qualified");
    }

    fn brief_with(segments: Vec<TargetSegment>, personas: &[&str], geography: &str) -> SearchBrief {
        SearchBrief {
            id: "b".into(),
            query: "q".into(),
            product_summary: "p".into(),
            customer_outcome: "o".into(),
            segments,
            geography: geography.into(),
            company_sizes: vec![],
            target_personas: personas.iter().map(|value| value.to_string()).collect(),
            positive_signals: vec![],
            exclusions: vec![],
            discovery_lanes: vec![],
            result_limit: 25,
            qualification_threshold: 70,
            quality_band: 4,
            approved: true,
            assumptions: vec![],
            interpreter: "manual_rules".into(),
            model_tag: None,
            model_digest: None,
            prompt_version: crate::models::SEARCH_PROMPT_VERSION.into(),
            created_at: "now".into(),
        }
    }

    #[test]
    fn generic_words_in_a_category_label_do_not_establish_the_segment() {
        let mut package = package();
        package.account.industry = None;
        package.evidence[0].title = "Brightline".into();
        package.evidence[0].excerpt =
            "We help professional services firms grow their brand.".into();
        let law = TargetSegment {
            id: "lawyer".into(),
            label: "Law firms".into(),
            discovery_category: Some("lawyer".into()),
        };
        let result = assess_for_search(&package, &brief_with(vec![law], &[], "United States"));
        assert!(!result
            .features
            .iter()
            .any(|f| f.rule_id == "segment:lawyer"));

        package.evidence[0].excerpt = "Contact our general manager today.".into();
        let general = TargetSegment {
            id: catalog::GENERAL_BUSINESS.into(),
            label: catalog::GENERAL_BUSINESS_LABEL.into(),
            discovery_category: None,
        };
        let result = assess_for_search(
            &package,
            &brief_with(vec![general.clone()], &[], "United States"),
        );
        assert_eq!(result.assessment.icp_fit, 20, "only geography, no segment");

        // A type-scoped discovery record does establish it.
        package.evidence[0].source_kind = "wikidata_record".into();
        let result = assess_for_search(&package, &brief_with(vec![general], &[], "United States"));
        assert!(result
            .features
            .iter()
            .any(|f| f.rule_id == "segment:general_business"));
    }

    #[test]
    fn personas_match_whole_words_only() {
        let mut package = package();
        package.evidence[0].excerpt =
            "Trusted by homeowners across Austin for twenty years.".into();
        assert_eq!(persona_score(&package, &["Owner".into()]), 0);
        package.evidence[0].excerpt = "Meet the owner, Maria Lopez, a master plumber.".into();
        assert_eq!(
            persona_score(&package, &["Owner".into()]),
            0,
            "a general company page is not people evidence"
        );
        package.evidence[0].source_kind = "persona_excerpt".into();
        assert_eq!(persona_score(&package, &["Owner".into()]), 65);
        package.people.push(crate::models::Person {
            id: "p".into(),
            account_id: "a".into(),
            name: "Unknown".into(),
            title: Some(String::new()),
        });
        package.evidence[0].excerpt = "Nothing relevant here at all.".into();
        assert_eq!(
            persona_score(&package, &["Owner".into()]),
            0,
            "an empty title matches nothing"
        );
    }

    #[test]
    fn exclusions_match_whole_phrases() {
        let mut package = package();
        package.evidence[0].excerpt = "Brunch favourites including avocado toast.".into();
        let mut brief = brief_with(vec![], &[], "United States");
        brief.exclusions = vec!["toast pos".into()];
        assert!(assess_for_search(&package, &brief)
            .assessment
            .hard_gates
            .iter()
            .all(|gate| gate.status != "fail"));
        package.evidence[0].excerpt = "Orders run through Toast POS at every register.".into();
        assert!(assess_for_search(&package, &brief)
            .assessment
            .hard_gates
            .iter()
            .any(|gate| gate.status == "fail"));
    }

    #[test]
    fn boilerplate_and_negated_hiring_is_not_a_signal() {
        let mut package = package();
        for excerpt in [
            "Join our team. Careers at Congress Avenue Bistro.",
            "We are not hiring at this time, but check back soon.",
        ] {
            package.evidence[0].excerpt = excerpt.into();
            assert!(
                matching_signal_evidence_ids(&package, &["we are hiring".into(), "hiring".into()])
                    .is_empty(),
                "{excerpt}"
            );
        }
        package.evidence[0].excerpt =
            "We're hiring servers and line cooks for weekend shifts.".into();
        assert_eq!(
            matching_signal_evidence_ids(&package, &["hiring".into()]),
            vec!["e".to_string()]
        );
    }

    #[test]
    fn one_sentence_cannot_satisfy_two_rules() {
        let mut package = package();
        package.evidence.push(package.evidence[0].clone());
        package.evidence[1].id = "e2".into();
        let mut brief = brief_with(vec![], &[], "United States");
        for (id, keyword) in [("first", "emergency"), ("second", "call now")] {
            brief
                .positive_signals
                .push(crate::models::QualificationRule {
                    id: id.into(),
                    label: id.into(),
                    component: "signal".into(),
                    keywords: vec![keyword.into()],
                    points: 45,
                    hard_gate: false,
                    origin: "pack".into(),
                });
        }
        let result = assess_for_search(&package, &brief);
        let awarded = result
            .features
            .iter()
            .filter(|feature| feature.component == "signal")
            .count();
        assert_eq!(
            awarded, 1,
            "two copies of one sentence are one piece of evidence"
        );
    }

    #[test]
    fn an_unmatched_required_rule_keeps_the_candidate_out_of_qualified() {
        let mut brief = brief_with(vec![], &[], "United States");
        brief.qualification_threshold = 10;
        brief
            .positive_signals
            .push(crate::models::QualificationRule {
                id: "after_hours".into(),
                label: "After hours".into(),
                component: "signal".into(),
                keywords: vec!["24 hour".into()],
                points: 100,
                hard_gate: false,
                origin: "pack".into(),
            });
        assert_eq!(assess_for_search(&package(), &brief).state, "qualified");
        brief
            .positive_signals
            .push(crate::models::QualificationRule {
                id: "portal".into(),
                label: "Patient portal".into(),
                component: "signal".into(),
                keywords: vec!["patient portal".into()],
                points: 45,
                hard_gate: true,
                origin: "pack".into(),
            });
        let result = assess_for_search(&package(), &brief);
        assert_eq!(result.state, "needs_review");
        assert!(result
            .rejection_reasons
            .iter()
            .any(|reason| reason.contains("Required signal not found")));
    }

    #[test]
    fn geography_compares_states_and_metros() {
        let mut package = package();
        package.account.location = Some("Haddonfield, New Jersey, United States".into());
        package.evidence[0].excerpt = "Serving patients from Philadelphia, Pennsylvania.".into();
        let gate = |package: &ProspectPackage, geography: &str| {
            assess_for_search(package, &brief_with(vec![], &[], geography))
                .assessment
                .hard_gates
                .iter()
                .find(|gate| gate.label.starts_with("Geography"))
                .unwrap()
                .status
                .clone()
        };
        assert_eq!(
            gate(&package, "Pennsylvania"),
            "unknown",
            "recorded location decides"
        );
        assert_eq!(gate(&package, "New Jersey"), "pass");
        package.account.location = Some("Austin metro, Texas, United States".into());
        assert_eq!(gate(&package, "Austin, TX"), "pass");
        assert_eq!(gate(&package, "Texas"), "pass");
        assert_eq!(gate(&package, "Houston"), "unknown");
        // "United States" as a bare location no longer satisfies a city request.
        package.account.location = Some("United States".into());
        package.evidence[0].excerpt = "Plumbing.".into();
        assert_eq!(gate(&package, "Austin, TX, United States"), "unknown");
    }

    #[test]
    fn an_application_written_title_never_counts_as_evidence() {
        let mut package = package();
        package.evidence[0].source_kind = "workflow_after_hours".into();
        package.evidence[0].title = "After-hours service evidence".into();
        package.evidence[0].excerpt =
            "We do not offer emergency service; all work is booked by email.".into();
        assert!(matching_signal_evidence_ids(
            &package,
            &["after hours".into(), "emergency service".into()]
        )
        .is_empty());
    }

    #[test]
    fn detected_technologies_count_as_signals_and_competitor_exclusions() {
        let mut package = package();
        package.evidence[0].source_kind = "technology_detected".into();
        package.evidence[0].title = "Website technology: Toast POS".into();
        package.evidence[0].excerpt = crate::technology::describe(
            crate::technology::find("toast").unwrap(),
            &url::Url::parse("https://acme.example/").unwrap(),
        );
        assert_eq!(
            matching_signal_evidence_ids(&package, &["toasttab".into()]),
            vec!["e".to_string()]
        );
        let mut brief = brief_with(vec![], &[], "United States");
        brief.exclusions = vec!["toast pos".into()];
        assert!(assess_for_search(&package, &brief)
            .assessment
            .hard_gates
            .iter()
            .any(|gate| gate.status == "fail"));
    }
}
