//! Qualification signals: what a prospect's own public pages would show if
//! they needed what the user sells.
//!
//! The planner used to choose from eight fixed rules, five of them about phone
//! intake, whatever the product. A payroll vendor and a POS vendor targeting
//! restaurants were scored on identical evidence. Rules now come from three
//! layers, all editable before a search runs:
//!
//! 1. **Packs** (this module): curated rule sets keyed to the kind of product
//!    the user describes. Offline and instant.
//! 2. **Model suggestions** (`ai::propose_signal_rules`): extra rules the local
//!    model proposes after the plan renders, sanitized here.
//! 3. **The user**, who can edit keywords, add rules, mark a rule required, or
//!    remove one.
//!
//! Every layer produces plain keyword rules that `qualification.rs` applies
//! deterministically. The model never scores a candidate.

use crate::catalog;
use crate::models::QualificationRule;

pub struct RuleTemplate {
    pub id: &'static str,
    pub label: &'static str,
    pub keywords: &'static [&'static str],
    pub points: i64,
    pub component: &'static str,
}

/// Points for a pack or model rule. Two independent hits, a type-proven
/// identity and a contact route clear the default threshold of 70; one hit
/// leaves a near match. `calibration` tests pin this.
pub const PACK_RULE_POINTS: i64 = 45;

#[rustfmt::skip]
pub const RULE_TEMPLATES: &[RuleTemplate] = &[
    // ---- Phone and intake workflow ----
    RuleTemplate { id: "after_hours", label: "After-hours or emergency service", keywords: &["24/7", "24 hour", "after hours", "emergency service"], points: 55, component: "signal" },
    RuleTemplate { id: "call_first", label: "Phone-first customer intake", keywords: &["call now", "call us", "phone consultation", "speak with"], points: 35, component: "signal" },
    RuleTemplate { id: "appointment", label: "Appointment or consultation workflow", keywords: &["schedule an appointment", "book by phone", "consultation", "request service"], points: 35, component: "signal" },
    RuleTemplate { id: "intake_hiring", label: "Intake, dispatch, or receptionist hiring", keywords: &["intake specialist", "dispatcher", "receptionist", "customer service representative"], points: 45, component: "signal" },
    RuleTemplate { id: "multiple_locations", label: "Multiple locations or service areas", keywords: &["\"locations\"", "service areas", "areas we serve"], points: 25, component: "icp" },
    // ---- Generic growth ----
    RuleTemplate { id: "hiring", label: "Relevant hiring activity", keywords: &["we are hiring", "open positions", "job openings"], points: 40, component: "signal" },
    RuleTemplate { id: "expansion", label: "Expansion activity", keywords: &["new location", "expanded", "now serving"], points: 45, component: "signal" },
    RuleTemplate { id: "recent_launch", label: "Recent product or service launch", keywords: &["launched", "introducing", "now offering"], points: 35, component: "signal" },
    // ---- Workforce ----
    RuleTemplate { id: "hourly_hiring", label: "Hiring hourly or front-line staff", keywords: &["now hiring", "we are hiring", "open positions", "job openings", "hiring servers", "hiring cooks", "hiring drivers", "hiring technicians", "apply online", "immediate openings"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "shift_workforce", label: "Shift-based or hourly workforce", keywords: &["shifts", "hourly", "part time", "full time", "team members", "crew members", "weekly pay", "flexible scheduling", "tips"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "benefits_offered", label: "Offers employee benefits", keywords: &["health insurance", "dental insurance", "401k", "paid time off", "benefits package", "employee benefits"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "volume_hiring", label: "Hiring at volume", keywords: &["multiple positions", "hiring for all positions", "immediate openings", "hiring event", "career fair", "positions available"], points: PACK_RULE_POINTS, component: "signal" },
    // ---- Sales channels ----
    RuleTemplate { id: "online_ordering", label: "Takes orders online or for delivery", keywords: &["order online", "online ordering", "pickup", "delivery", "doordash", "uber eats", "grubhub", "catering orders"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "in_person_sales", label: "In-person sales or service counter", keywords: &["dine in", "walk ins", "visit our store", "in store", "our menu", "showroom"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "loyalty", label: "Gift cards, loyalty or rewards", keywords: &["gift cards", "loyalty program", "rewards program", "rewards members"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "sells_online", label: "Sells products online", keywords: &["shop online", "online store", "add to cart", "free shipping", "buy online", "shop now", "new arrivals"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "online_booking", label: "Online booking or quote requests", keywords: &["book online", "schedule online", "request a quote", "free estimate", "get a quote", "book now"], points: PACK_RULE_POINTS, component: "signal" },
    // ---- Marketing ----
    RuleTemplate { id: "reviews_reliance", label: "Relies on reviews and reputation", keywords: &["reviews", "testimonials", "five star", "5 star", "google reviews", "yelp"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "promotions", label: "Runs promotions or specials", keywords: &["specials", "promotions", "coupons", "limited time offer", "discount"], points: PACK_RULE_POINTS, component: "signal" },
    // ---- Regulated data ----
    RuleTemplate { id: "client_data_online", label: "Collects patient or client data online", keywords: &["patient portal", "client portal", "secure portal", "intake forms", "online forms", "new patient forms", "electronic health records", "electronic records", "upload documents", "pay your bill online"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "telehealth", label: "Remote or telehealth services", keywords: &["telehealth", "telemedicine", "virtual visits", "video consultations", "virtual consultation"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "insurance_billing", label: "Bills insurance", keywords: &["insurance plans", "accept insurance", "insurance accepted", "dental insurance", "medical insurance", "in network", "medicare", "medicaid"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "admin_hiring", label: "Hiring office or compliance staff", keywords: &["front office", "office manager", "billing coordinator", "compliance officer", "front desk", "administrative assistant"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "compliance_obligations", label: "Names a compliance obligation", keywords: &["hipaa", "pci", "soc 2", "cmmc", "gdpr", "regulatory compliance"], points: PACK_RULE_POINTS, component: "signal" },
    // ---- Technology and operations ----
    RuleTemplate { id: "it_hiring", label: "Hiring IT staff", keywords: &["it manager", "systems administrator", "network administrator", "help desk", "it support specialist"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "distributed_team", label: "Distributed or remote team", keywords: &["remote employees", "remote team", "hybrid", "\"offices\"", "work from home"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "invoicing", label: "Invoices customers or offers financing", keywords: &["pay your invoice", "invoices", "payment plans", "financing available", "net 30", "billing"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "fleet_operations", label: "Runs a vehicle fleet", keywords: &["fleet", "trucks", "vans", "service vehicles", "drivers"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "delivery_service", label: "Delivers or ships to customers", keywords: &["same day delivery", "we deliver", "local delivery", "shipping", "delivery area"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "driver_hiring", label: "Hiring drivers", keywords: &["hiring drivers", "cdl", "driver positions", "delivery drivers"], points: PACK_RULE_POINTS, component: "signal" },
    // ---- Representation (creators seeking agents) ----
    RuleTemplate { id: "accepting_submissions", label: "Accepting submissions or queries", keywords: &["accepting submissions", "accepting query", "query letters", "submission guidelines", "submissions", "open to queries"], points: PACK_RULE_POINTS, component: "signal" },
    RuleTemplate { id: "represents_creators", label: "Represents writers or creators", keywords: &["represent writers", "represents screenwriters", "screenwriters", "television writers", "authors", "literary clients", "literary department", "showrunners"], points: PACK_RULE_POINTS, component: "signal" },
];

pub fn template(id: &str) -> Option<&'static RuleTemplate> {
    RULE_TEMPLATES.iter().find(|template| template.id == id)
}

pub fn rule_from_template(template: &RuleTemplate, origin: &str) -> QualificationRule {
    QualificationRule {
        id: template.id.into(),
        label: template.label.into(),
        component: template.component.into(),
        keywords: template
            .keywords
            .iter()
            .map(|value| value.to_string())
            .collect(),
        points: template.points,
        hard_gate: false,
        origin: origin.into(),
    }
}

pub struct Pack {
    pub id: &'static str,
    pub label: &'static str,
    /// Whole-word phrases in the user's description that select this pack.
    pub triggers: &'static [&'static str],
    pub outcome: &'static str,
    pub personas: &'static [&'static str],
    pub rules: &'static [&'static str],
}

#[rustfmt::skip]
pub const PACKS: &[Pack] = &[
    Pack { id: "voice", label: "phone and intake", triggers: &["voice", "phone", "call", "calls", "answering", "receptionist", "intake", "call center"], outcome: "Capture and route valuable inbound calls more consistently.", personas: &["Owner", "General Manager", "Operations", "Office Manager", "Intake", "Dispatch"], rules: &["after_hours", "call_first", "appointment", "intake_hiring", "multiple_locations"] },
    Pack { id: "workforce", label: "payroll, scheduling and HR", triggers: &["payroll", "scheduling", "shift", "shifts", "workforce", "hr", "hris", "timekeeping", "time tracking", "time clock", "employee scheduling", "human resources"], outcome: "Pay, schedule and manage an hourly workforce with less manual effort.", personas: &["Owner", "General Manager", "Operations", "HR", "Office Manager", "Controller"], rules: &["hourly_hiring", "shift_workforce", "multiple_locations", "benefits_offered"] },
    Pack { id: "commerce", label: "point of sale, ordering and payments", triggers: &["pos", "point of sale", "payments", "payment processing", "online ordering", "ordering", "merchant services", "card processing", "kiosk", "loyalty"], outcome: "Take orders and payments across every channel with less friction.", personas: &["Owner", "General Manager", "Operations"], rules: &["online_ordering", "in_person_sales", "loyalty", "multiple_locations"] },
    Pack { id: "marketing", label: "marketing, SEO and reputation", triggers: &["marketing", "seo", "advertising", "ads", "reviews", "reputation", "social media", "lead generation", "branding", "google ads"], outcome: "Win more customers from search, reviews and advertising.", personas: &["Owner", "Marketing Manager", "General Manager"], rules: &["reviews_reliance", "online_booking", "promotions", "expansion"] },
    Pack { id: "ecommerce", label: "websites and ecommerce", triggers: &["ecommerce", "e commerce", "online store", "shopify", "web design", "website design", "checkout", "storefront"], outcome: "Sell more online with a faster, better-converting store.", personas: &["Owner", "Ecommerce Manager", "Marketing Manager"], rules: &["sells_online", "promotions", "recent_launch"] },
    Pack { id: "it", label: "IT, managed services and security", triggers: &["managed services", "msp", "it support", "cybersecurity", "network security", "data security", "security", "cloud", "helpdesk", "help desk", "backup", "endpoint"], outcome: "Keep systems secure and running without a full in-house IT team.", personas: &["Owner", "Office Manager", "IT Manager", "Operations", "CFO"], rules: &["distributed_team", "client_data_online", "it_hiring", "compliance_obligations"] },
    Pack { id: "compliance", label: "compliance and regulated data", triggers: &["hipaa", "compliance", "patient data", "phi", "regulatory", "audit", "soc 2", "pci", "privacy"], outcome: "Meet regulatory obligations for sensitive client data with less effort.", personas: &["Owner", "Practice Manager", "Office Manager", "Compliance Officer", "Administrator"], rules: &["client_data_online", "insurance_billing", "telehealth", "admin_hiring"] },
    Pack { id: "finance", label: "bookkeeping and finance", triggers: &["bookkeeping", "accounting", "accounts payable", "accounts receivable", "invoicing", "billing software", "fractional cfo", "cfo services", "tax preparation"], outcome: "Keep the books, billing and cash flow under control.", personas: &["Owner", "CFO", "Controller", "Office Manager"], rules: &["invoicing", "expansion", "hiring", "multiple_locations"] },
    Pack { id: "recruiting", label: "recruiting and staffing", triggers: &["recruiting", "staffing", "applicant tracking", "ats", "talent acquisition", "job board", "background checks", "hiring software"], outcome: "Fill open roles faster.", personas: &["Owner", "HR Manager", "Recruiter", "Operations"], rules: &["hourly_hiring", "volume_hiring", "expansion"] },
    Pack { id: "benefits", label: "insurance and employee benefits", triggers: &["insurance", "benefits", "health insurance", "workers comp", "workers compensation", "liability insurance", "401k", "retirement plan", "employee benefits"], outcome: "Protect the business and its people at a fair cost.", personas: &["Owner", "HR Manager", "CFO", "Office Manager"], rules: &["hourly_hiring", "shift_workforce", "fleet_operations", "multiple_locations"] },
    Pack { id: "logistics", label: "logistics and fleet", triggers: &["logistics", "fleet", "delivery", "routing", "route planning", "dispatch software", "gps tracking", "freight", "telematics"], outcome: "Run deliveries and vehicles more efficiently.", personas: &["Owner", "Operations Manager", "Fleet Manager", "Dispatch"], rules: &["fleet_operations", "delivery_service", "driver_hiring", "multiple_locations"] },
    Pack { id: "representation", label: "agents and representation", triggers: &["representation", "literary agent", "literary agents", "literary agency", "literary manager", "talent agent", "talent agency", "screenwriter", "screenplay", "manuscript"], outcome: "Find representatives who take on work like yours.", personas: &["Agent", "Literary Manager", "Partner"], rules: &["represents_creators", "accepting_submissions"] },
];

/// Packs selected by whole-word triggers in the description, most triggers
/// first, so the dominant theme of the request leads.
pub fn match_packs(query: &str) -> Vec<&'static Pack> {
    let normalized = catalog::normalize(query);
    let mut scored = PACKS
        .iter()
        .filter_map(|pack| {
            let hits = pack
                .triggers
                .iter()
                .filter(|trigger| catalog::contains_word(&normalized, trigger))
                .count();
            (hits > 0).then_some((hits, pack))
        })
        .collect::<Vec<_>>();
    scored.sort_by_key(|(hits, _)| std::cmp::Reverse(*hits));
    scored.into_iter().map(|(_, pack)| pack).collect()
}

/// Words that say nothing about a particular business on their own.
const GENERIC_TERMS: &[&str] = &[
    "service",
    "services",
    "quality",
    "customer",
    "customers",
    "client",
    "clients",
    "contact",
    "team",
    "solution",
    "solutions",
    "company",
    "business",
    "home",
    "about",
    "welcome",
    "best",
    "professional",
    "local",
    "free",
    "more",
    "new",
    "our",
    "we",
    "you",
    "the",
    "and",
    "for",
    "us",
    "with",
    "your",
    "help",
    "great",
];

const MAX_RULES: usize = 10;
const MAX_KEYWORDS: usize = 10;
const MAX_KEYWORD_WORDS: usize = 5;

fn clean_keyword(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let quoted = trimmed.len() > 2 && trimmed.starts_with('"') && trimmed.ends_with('"');
    let inner = if quoted {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    let cleaned = inner
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || " '/&-".contains(character) {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let words = cleaned.split(' ').count();
    if !(3..=40).contains(&cleaned.len()) || words > MAX_KEYWORD_WORDS {
        return None;
    }
    if cleaned.split(' ').all(|word| GENERIC_TERMS.contains(&word)) {
        return None;
    }
    Some(if quoted {
        format!("\"{cleaned}\"")
    } else {
        cleaned
    })
}

fn slug(value: &str) -> String {
    let slug = value
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    slug.chars().take(40).collect()
}

/// Bound and clean rules from any origin before they are stored or applied.
///
/// Rules arrive from the plan editor and from the local model, so nothing
/// about their shape is trusted: keywords are reduced to short plain phrases,
/// generic words that would match every site are dropped, points are clamped,
/// ids are made unique, and empty rules disappear.
pub fn sanitize_rules(rules: &mut Vec<QualificationRule>) {
    let mut ids = std::collections::HashSet::new();
    let mut kept = Vec::new();
    for mut rule in std::mem::take(rules) {
        rule.label = rule
            .label
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(60)
            .collect();
        let mut seen = std::collections::HashSet::new();
        rule.keywords = rule
            .keywords
            .iter()
            .filter_map(|keyword| clean_keyword(keyword))
            .filter(|keyword| seen.insert(keyword.clone()))
            .take(MAX_KEYWORDS)
            .collect();
        if rule.label.is_empty() || rule.keywords.is_empty() {
            continue;
        }
        rule.points = rule.points.clamp(0, 100);
        if !matches!(rule.component.as_str(), "signal" | "icp") {
            rule.component = "signal".into();
        }
        if rule.origin.trim().is_empty() {
            rule.origin = "user".into();
        }
        let base = {
            let candidate = slug(&rule.id);
            if candidate.is_empty() {
                slug(&rule.label)
            } else {
                candidate
            }
        };
        let mut id = base.clone();
        let mut suffix = 2;
        while !ids.insert(id.clone()) {
            id = format!("{base}_{suffix}");
            suffix += 1;
        }
        rule.id = id;
        kept.push(rule);
        if kept.len() == MAX_RULES {
            break;
        }
    }
    *rules = kept;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pack_rule_has_a_template_and_every_template_keyword_survives_sanitizing() {
        for pack in PACKS {
            for id in pack.rules {
                assert!(
                    template(id).is_some(),
                    "{} names unknown rule {id}",
                    pack.id
                );
            }
        }
        for template in RULE_TEMPLATES {
            let mut rules = vec![rule_from_template(template, "pack")];
            sanitize_rules(&mut rules);
            assert_eq!(
                rules[0].keywords.len(),
                template.keywords.len(),
                "{} lost keywords to sanitizing",
                template.id
            );
        }
    }

    #[test]
    fn packs_match_whole_words() {
        let ids = |query: &str| {
            match_packs(query)
                .iter()
                .map(|pack| pack.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("payroll and shift scheduling software")[0], "workforce");
        assert_eq!(
            ids("HIPAA compliance and patient data security")[0],
            "compliance"
        );
        assert!(
            ids("a postal service").is_empty(),
            "pos is not a substring match"
        );
        assert!(ids("I want representation as a screenwriter").contains(&"representation"));
    }

    #[test]
    fn sanitizing_bounds_and_cleans_untrusted_rules() {
        let mut rules = vec![
            QualificationRule {
                id: "".into(),
                label: "  Patient   portal ".into(),
                component: "score".into(),
                keywords: vec![
                    "Patient Portal".into(),
                    "patient portal".into(),
                    "services".into(),
                    "our team".into(),
                    "a phrase that is much too long to be a keyword at all".into(),
                    "}; DROP".into(),
                    "\"Locations\"".into(),
                ],
                points: 900,
                hard_gate: true,
                origin: "".into(),
            },
            QualificationRule {
                id: "patient_portal".into(),
                label: "Duplicate id".into(),
                component: "signal".into(),
                keywords: vec!["telehealth".into()],
                points: -5,
                hard_gate: false,
                origin: "model".into(),
            },
            QualificationRule {
                id: "empty".into(),
                label: "Nothing usable".into(),
                component: "signal".into(),
                keywords: vec!["the".into(), "services".into()],
                points: 45,
                hard_gate: false,
                origin: "model".into(),
            },
        ];
        sanitize_rules(&mut rules);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].label, "Patient portal");
        assert_eq!(rules[0].id, "patient_portal");
        assert_eq!(rules[0].component, "signal");
        assert_eq!(rules[0].points, 100);
        assert_eq!(rules[0].origin, "user");
        assert_eq!(
            rules[0].keywords,
            vec!["patient portal", "drop", "\"locations\""]
        );
        assert_eq!(rules[1].id, "patient_portal_2");
        assert_eq!(rules[1].points, 0);
    }
}
