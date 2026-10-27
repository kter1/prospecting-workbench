//! Offline relevance evaluation.
//!
//! Each case is a prospecting request plus a small corpus of realistic company
//! sites, hand-labelled for whether a person making that request would want
//! them. Every site runs through the real planner, evidence extraction and
//! scoring, so a change to any of them shows up here as a change in precision
//! or recall rather than as an anecdote.
//!
//! The near-miss sites are the point. Each one carries a phrase that fooled an
//! earlier version of the matcher: boilerplate "join our team", "we are not
//! hiring", "homeowners", "avocado toast", a generic word from a category label.

use url::Url;

use crate::models::{Account, ContactPoint, Evidence, ProspectPackage, SearchBrief};
use crate::qualification::assess_for_search;
use crate::research::{extract_facts, ResearchFact};
use crate::safety::FetchResult;

pub struct Site {
    pub name: &'static str,
    pub location: &'static str,
    /// The discovery record that would have introduced this candidate.
    pub record: (&'static str, &'static str),
    pub relevant: bool,
    /// (path, title, page text)
    pub pages: &'static [(&'static str, &'static str, &'static str)],
}

pub struct Case {
    pub name: &'static str,
    pub query: &'static str,
    pub geography: &'static str,
    pub sites: &'static [Site],
}

pub struct CaseResult {
    pub qualified: Vec<&'static str>,
    pub precision: f64,
    pub recall: f64,
}

fn slug(name: &str) -> String {
    name.to_ascii_lowercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

pub fn package_for(site: &Site, brief: &SearchBrief) -> ProspectPackage {
    let domain = format!("{}.example", slug(site.name));
    let account = Account {
        id: slug(site.name),
        name: site.name.into(),
        domain: Some(domain.clone()),
        website_url: Some(format!("https://{domain}/")),
        industry: None,
        employee_band: None,
        location: Some(site.location.into()),
        status: "researched".into(),
        score: None,
        score_label: None,
        researched_at: None,
        created_at: "now".into(),
        saved_at: None,
        favorited_at: None,
        trashed_at: None,
    };
    let mut facts: Vec<ResearchFact> = Vec::new();
    for (path, title, text) in site.pages {
        let page = FetchResult {
            final_url: Url::parse(&format!("https://{domain}{path}")).unwrap(),
            title: (*title).into(),
            text: (*text).into(),
            links: vec![],
            etag: None,
            modified_at: None,
        };
        facts.extend(extract_facts(&account, page));
        let _ = brief;
    }
    let mut evidence = vec![Evidence {
        id: format!("{}-record", account.id),
        account_id: account.id.clone(),
        source_url: format!("https://records.example/{}", account.id),
        source_kind: site.record.0.into(),
        title: "Discovery record".into(),
        excerpt: format!("{}: {}", site.name, site.record.1),
        retrieved_at: "now".into(),
        published_at: None,
        content_hash: "record".into(),
        confidence: "medium".into(),
        freshness: "fresh".into(),
        quality_flags: vec![],
    }];
    let mut contacts = Vec::new();
    for (index, fact) in facts.into_iter().enumerate() {
        let id = format!("{}-{index}", account.id);
        for (kind, value, confidence) in &fact.contacts {
            contacts.push(ContactPoint {
                id: format!("{id}-{value}"),
                account_id: account.id.clone(),
                person_id: None,
                kind: kind.clone(),
                value: value.clone(),
                evidence_id: id.clone(),
                confidence: confidence.clone(),
            });
        }
        evidence.push(Evidence {
            id,
            account_id: account.id.clone(),
            source_url: fact.source_url,
            source_kind: fact.source_kind,
            title: fact.title,
            excerpt: fact.excerpt,
            retrieved_at: "now".into(),
            published_at: None,
            content_hash: format!("h{index}"),
            confidence: fact.confidence,
            freshness: "fresh".into(),
            quality_flags: vec![],
        });
    }
    ProspectPackage {
        account,
        people: vec![],
        contacts,
        evidence,
        signals: vec![],
        assessment: None,
        draft: None,
        call_brief: None,
        suppressed: false,
        latest_outcome: None,
        discovery_context: None,
    }
}

pub fn evaluate(case: &Case, brief: &SearchBrief, verbose: bool) -> CaseResult {
    let mut qualified = Vec::new();
    for site in case.sites {
        let package = package_for(site, brief);
        let result = assess_for_search(&package, brief);
        if verbose {
            println!(
                "  {:<7} {:<5} {:>3}  {:<34} {:?}",
                result.state,
                if site.relevant { "want" } else { "skip" },
                result.assessment.total,
                site.name,
                result
                    .features
                    .iter()
                    .map(|feature| feature.rule_id.as_str())
                    .collect::<Vec<_>>()
            );
        }
        if result.state == "qualified" {
            qualified.push(site.name);
        }
    }
    let relevant = case.sites.iter().filter(|site| site.relevant).count();
    let true_positive = case
        .sites
        .iter()
        .filter(|site| site.relevant && qualified.contains(&site.name))
        .count();
    CaseResult {
        precision: if qualified.is_empty() {
            0.0
        } else {
            true_positive as f64 / qualified.len() as f64
        },
        recall: if relevant == 0 {
            1.0
        } else {
            true_positive as f64 / relevant as f64
        },
        qualified,
    }
}

const OSM_RESTAURANT: (&str, &str) = (
    "openstreetmap_record",
    "Mapped restaurant in the bounded Austin metro sample; location established by the bounded search area, not a published address.",
);
const OSM_DENTIST: (&str, &str) = (
    "openstreetmap_record",
    "Mapped dentist in the bounded Philadelphia metro sample; location established by the bounded search area, not a published address.",
);
const OSM_PLUMBER: (&str, &str) = (
    "openstreetmap_record",
    "Mapped plumber in the bounded Philadelphia metro sample; location established by the bounded search area, not a published address.",
);
const WIKIDATA_AGENCY: (&str, &str) = (
    "wikidata_record",
    "Recorded in Wikidata as a talent agency operating in the United States.",
);

pub const CASES: &[Case] = &[
    Case {
        name: "payroll software for restaurants",
        query: "I sell payroll and shift scheduling software to restaurants with hourly staff",
        geography: "Austin, TX",
        sites: &[
            Site {
                name: "Barton Taqueria Group",
                location: "Austin metro, Texas, United States",
                record: OSM_RESTAURANT,
                relevant: true,
                pages: &[
                    ("/", "Barton Taqueria", "Barton Taqueria serves fresh tacos at three Austin locations.\nVisit us on South Lamar, East Sixth and in Round Rock.\nQuestions? Email hello@bartontaqueriagroup.example"),
                    ("/careers", "Careers at Barton Taqueria", "We're hiring servers, line cooks and dishwashers at all locations.\nFlexible shifts, weekly pay and tips for every hourly team member.\nApply online today."),
                ],
            },
            Site {
                name: "Lamar Street Kitchen",
                location: "Austin metro, Texas, United States",
                record: OSM_RESTAURANT,
                relevant: true,
                pages: &[
                    ("/", "Lamar Street Kitchen", "Neighborhood kitchen and bar open seven days a week.\nWe now have two locations: downtown and the Domain.\nReservations: info@lamarstreetkitchen.example"),
                    ("/jobs", "Jobs", "Now hiring experienced bartenders and hosts for evening shifts.\nPart-time and full-time hourly positions available."),
                ],
            },
            Site {
                name: "Pecan Grove Diner",
                location: "Austin metro, Texas, United States",
                record: OSM_RESTAURANT,
                relevant: true,
                pages: &[
                    ("/", "Pecan Grove Diner", "Breakfast and lunch since 1987, family owned and operated.\nOur crew of 40 team members keeps the griddle hot from 6am.\nContact: hello@pecangrovediner.example"),
                    ("/careers", "Careers", "Open positions: line cook, server, busser. Weekly pay, flexible scheduling.\nWe are hiring for weekend shifts now."),
                ],
            },
            Site {
                name: "Avocado Toast Cafe",
                location: "Austin metro, Texas, United States",
                record: OSM_RESTAURANT,
                relevant: false,
                pages: &[
                    ("/", "Avocado Toast Cafe", "A tiny counter serving avocado toast and cold brew.\nRun by the two of us, Sam and Jo, since 2019.\nWe have one location on Cesar Chavez.\nSay hi: hello@avocadotoastcafe.example"),
                    ("/careers", "Careers", "Join our team! We are not hiring at this time, but check back soon.\nThanks for your interest in working with us."),
                ],
            },
            Site {
                name: "Hill Country Food Truck",
                location: "Austin metro, Texas, United States",
                record: OSM_RESTAURANT,
                relevant: false,
                pages: &[
                    ("/", "Hill Country Food Truck", "Smoked brisket tacos from our single truck on Rainey Street.\nFollow us for daily specials and catering requests.\nCatering: info@hillcountryfoodtruck.example"),
                ],
            },
            Site {
                name: "Congress Avenue Bistro",
                location: "Austin metro, Texas, United States",
                record: OSM_RESTAURANT,
                relevant: false,
                pages: &[
                    ("/", "Congress Avenue Bistro", "French bistro dishes served in a cozy dining room.\nSee our menu, wine list and private dining options.\nReservations: info@congressavenuebistro.example"),
                    ("/careers", "Careers", "Join our team. Careers at Congress Avenue Bistro.\nWe love working with passionate people."),
                ],
            },
        ],
    },
    Case {
        name: "HIPAA compliance for dental practices",
        query: "We sell HIPAA compliance and patient data security software to dental practices",
        geography: "Pennsylvania",
        sites: &[
            Site {
                name: "Schuylkill Family Dental",
                location: "Philadelphia metro, Pennsylvania, United States",
                record: OSM_DENTIST,
                relevant: true,
                pages: &[
                    ("/", "Schuylkill Family Dental", "Comprehensive family dentistry with offices in Manayunk and Conshohocken.\nNew patients can complete intake forms online before their visit.\nEmail frontdesk@schuylkillfamilydental.example"),
                    ("/patients", "Patient Information", "Use our secure patient portal to view records and pay your bill.\nWe accept most major dental insurance plans."),
                ],
            },
            Site {
                name: "Main Line Orthodontics",
                location: "Philadelphia metro, Pennsylvania, United States",
                record: OSM_DENTIST,
                relevant: true,
                pages: &[
                    ("/", "Main Line Orthodontics", "Braces and clear aligners for children and adults across four locations.\nRequest a consultation and complete your new patient forms online.\nContact office@mainlineorthodontics.example"),
                    ("/careers", "Careers", "We are hiring a dental assistant and a front office coordinator.\nExperience with electronic health records is a plus."),
                ],
            },
            Site {
                name: "Fishtown Dental Studio",
                location: "Philadelphia metro, Pennsylvania, United States",
                record: OSM_DENTIST,
                relevant: true,
                pages: &[
                    ("/", "Fishtown Dental Studio", "Modern general dentistry on Frankford Avenue.\nOur patient portal lets you book appointments and update insurance details.\nTelehealth consultations are available for new patients."),
                ],
            },
            Site {
                name: "Delaware Valley Dental Supply",
                location: "Philadelphia metro, Pennsylvania, United States",
                record: OSM_DENTIST,
                relevant: false,
                pages: &[
                    ("/", "Delaware Valley Dental Supply", "Wholesale dental supplies, gloves and instruments for professionals.\nFree shipping on orders over $200.\nOrder line hours: weekdays 9 to 5."),
                ],
            },
            Site {
                name: "Smile Growth Marketing",
                location: "Philadelphia metro, Pennsylvania, United States",
                record: OSM_DENTIST,
                relevant: false,
                pages: &[
                    ("/", "Smile Growth Marketing", "We help dental practices and professional services firms grow with SEO.\nOur general manager works with every client personally.\nBook a strategy call with our team."),
                ],
            },
            Site {
                name: "Haddonfield Dental Arts",
                location: "Haddonfield, New Jersey, United States",
                record: (
                    "openstreetmap_record",
                    "Mapped dentist in Haddonfield, New Jersey, United States.",
                ),
                relevant: false,
                pages: &[
                    ("/", "Haddonfield Dental Arts", "Cosmetic dentistry in Haddonfield, New Jersey.\nNew patients can complete forms online through our secure patient portal.\nWe accept dental insurance."),
                ],
            },
        ],
    },
    Case {
        name: "screenwriter seeking representation",
        query: "I am a screenwriter with a pilot script looking for literary representation",
        geography: "United States",
        sites: &[
            Site {
                name: "Sunset Literary Management",
                location: "Los Angeles, United States",
                record: WIKIDATA_AGENCY,
                relevant: true,
                pages: &[
                    ("/", "Sunset Literary Management", "Sunset Literary Management represents screenwriters and television writers.\nWe are currently accepting query letters for original pilot scripts.\nSubmissions: queries@sunsetliterarymanagement.example"),
                ],
            },
            Site {
                name: "Westside Talent Partners",
                location: "Los Angeles, United States",
                record: WIKIDATA_AGENCY,
                relevant: true,
                pages: &[
                    ("/", "Westside Talent Partners", "A boutique agency representing writers, directors and showrunners.\nOur literary department reviews screenplay submissions year round.\nContact info@westsidetalentpartners.example"),
                ],
            },
            Site {
                name: "Hollywood Models Agency",
                location: "Los Angeles, United States",
                record: WIKIDATA_AGENCY,
                relevant: false,
                pages: &[
                    ("/", "Hollywood Models Agency", "Fashion and commercial modeling representation since 1990.\nWe are hiring a booking coordinator for our new location.\nOpen calls for models every Tuesday."),
                ],
            },
            Site {
                name: "Pacific Sports Management",
                location: "Los Angeles, United States",
                record: WIKIDATA_AGENCY,
                relevant: false,
                pages: &[
                    ("/", "Pacific Sports Management", "Athlete representation for professional baseball and basketball players.\nWe recently launched a new name, image and likeness practice.\nNow serving college athletes nationwide."),
                ],
            },
        ],
    },
    Case {
        name: "voice intake for plumbers",
        query: "I sell an AI voice receptionist that answers calls for plumbers",
        geography: "Pennsylvania",
        sites: &[
            Site {
                name: "Liberty Plumbing and Drain",
                location: "Philadelphia metro, Pennsylvania, United States",
                record: OSM_PLUMBER,
                relevant: true,
                pages: &[
                    ("/", "Liberty Plumbing and Drain", "24/7 emergency plumbing service across Philadelphia.\nCall now and a licensed plumber will be on the way.\nService areas: Philadelphia, Delaware County and Montgomery County.\nEmail dispatch@libertyplumbinganddrain.example"),
                ],
            },
            Site {
                name: "Keystone Rooter",
                location: "Philadelphia metro, Pennsylvania, United States",
                record: OSM_PLUMBER,
                relevant: true,
                pages: &[
                    ("/", "Keystone Rooter", "Drain cleaning and water heater repair, open 24 hours a day.\nGive us a call to schedule an appointment with a technician.\nContact office@keystonerooter.example"),
                    ("/careers", "Careers", "We are hiring a dispatcher to answer customer calls and schedule jobs."),
                ],
            },
            Site {
                name: "Old City Pipe Works",
                location: "Philadelphia metro, Pennsylvania, United States",
                record: OSM_PLUMBER,
                relevant: false,
                pages: &[
                    ("/", "Old City Pipe Works", "Plumbing services for homeowners in Old City since 1962.\nWe do not offer emergency service; all work is booked by email.\nEmail jobs@oldcitypipeworks.example"),
                ],
            },
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    async fn brief_for(case: &Case) -> SearchBrief {
        let mut brief =
            crate::ai::interpret_search_brief(case.query, None, case.geography, 25, None)
                .await
                .unwrap();
        brief.approved = true;
        brief
    }

    /// Prints the per-site table. Run with `--nocapture` to read it.
    #[tokio::test]
    async fn relevance_report() {
        for case in CASES {
            let brief = brief_for(case).await;
            println!(
                "\n{} — rules: {:?}",
                case.name,
                brief
                    .positive_signals
                    .iter()
                    .map(|rule| rule.id.as_str())
                    .collect::<Vec<_>>()
            );
            let result = evaluate(case, &brief, true);
            println!(
                "  precision {:.2}  recall {:.2}  qualified {:?}",
                result.precision, result.recall, result.qualified
            );
        }
    }
}
