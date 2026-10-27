//! Website technology detection from page markup.
//!
//! Which booking, ordering, commerce or marketing tools a business runs is
//! some of the most useful prospecting evidence there is, and paid data
//! vendors sell exactly this. Most of it is visible for free in the markup the
//! business publishes: the scripts it loads, the stylesheets it links, the
//! booking and ordering links it gives customers, and the generator it
//! declares.
//!
//! Detection reads only attribute values and the generator tag, never visible
//! text, so a page that merely *mentions* Shopify is not recorded as running
//! it. Every name, host and category in the resulting evidence comes from the
//! static table below, so page content can never put words into an excerpt.

use std::sync::LazyLock;

use regex::Regex;
use url::Url;

pub struct Technology {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    /// Hosts (matched on a dot boundary) or path fragments starting with '/'.
    pub needles: &'static [&'static str],
    /// Case-insensitive fragments of a `<meta name="generator">` value.
    pub generators: &'static [&'static str],
}

#[rustfmt::skip]
pub const TECHNOLOGIES: &[Technology] = &[
    Technology { id: "shopify", name: "Shopify", category: "ecommerce platform", needles: &["cdn.shopify.com", "myshopify.com"], generators: &["shopify"] },
    Technology { id: "wordpress", name: "WordPress", category: "website platform", needles: &["/wp-content/", "/wp-includes/"], generators: &["wordpress"] },
    Technology { id: "wix", name: "Wix", category: "website platform", needles: &["static.wixstatic.com", "parastorage.com"], generators: &["wix.com"] },
    Technology { id: "squarespace", name: "Squarespace", category: "website platform", needles: &["static1.squarespace.com", "squarespace-cdn.com"], generators: &["squarespace"] },
    Technology { id: "webflow", name: "Webflow", category: "website platform", needles: &["website-files.com"], generators: &["webflow"] },
    Technology { id: "hubspot", name: "HubSpot", category: "marketing automation", needles: &["js.hs-scripts.com", "js.hsforms.net", "js.hs-analytics.net"], generators: &[] },
    Technology { id: "intercom", name: "Intercom", category: "live chat", needles: &["widget.intercom.io", "js.intercomcdn.com"], generators: &[] },
    Technology { id: "zendesk", name: "Zendesk", category: "customer support", needles: &["static.zdassets.com", "zendesk.com"], generators: &[] },
    Technology { id: "calendly", name: "Calendly", category: "online scheduling", needles: &["calendly.com"], generators: &[] },
    Technology { id: "acuity", name: "Acuity Scheduling", category: "online scheduling", needles: &["acuityscheduling.com"], generators: &[] },
    Technology { id: "toast", name: "Toast POS", category: "restaurant point of sale and online ordering", needles: &["toasttab.com"], generators: &[] },
    Technology { id: "square", name: "Square", category: "point of sale and payments", needles: &["squareup.com", "square.site"], generators: &[] },
    Technology { id: "stripe", name: "Stripe", category: "online payments", needles: &["js.stripe.com"], generators: &[] },
    Technology { id: "opentable", name: "OpenTable", category: "restaurant reservations", needles: &["opentable.com"], generators: &[] },
    Technology { id: "resy", name: "Resy", category: "restaurant reservations", needles: &["resy.com"], generators: &[] },
    Technology { id: "mindbody", name: "Mindbody", category: "class and appointment booking", needles: &["mindbodyonline.com", "healcode.com"], generators: &[] },
    Technology { id: "servicetitan", name: "ServiceTitan", category: "field service management", needles: &["servicetitan.com"], generators: &[] },
    Technology { id: "housecallpro", name: "Housecall Pro", category: "field service management", needles: &["housecallpro.com"], generators: &[] },
    Technology { id: "jobber", name: "Jobber", category: "field service management", needles: &["getjobber.com"], generators: &[] },
    Technology { id: "podium", name: "Podium", category: "reviews and messaging", needles: &["podium.com"], generators: &[] },
    Technology { id: "birdeye", name: "Birdeye", category: "reviews and reputation", needles: &["birdeye.com"], generators: &[] },
    Technology { id: "nexhealth", name: "NexHealth", category: "patient scheduling and forms", needles: &["nexhealth.com"], generators: &[] },
    Technology { id: "zocdoc", name: "Zocdoc", category: "patient booking", needles: &["zocdoc.com"], generators: &[] },
    Technology { id: "klaviyo", name: "Klaviyo", category: "email marketing", needles: &["klaviyo.com"], generators: &[] },
    Technology { id: "mailchimp", name: "Mailchimp", category: "email marketing", needles: &["list-manage.com", "chimpstatic.com"], generators: &[] },
    Technology { id: "greenhouse", name: "Greenhouse", category: "applicant tracking", needles: &["greenhouse.io"], generators: &[] },
    Technology { id: "lever", name: "Lever", category: "applicant tracking", needles: &["lever.co"], generators: &[] },
    Technology { id: "bamboohr", name: "BambooHR", category: "HR software", needles: &["bamboohr.com"], generators: &[] },
];

pub fn find(id: &str) -> Option<&'static Technology> {
    TECHNOLOGIES.iter().find(|technology| technology.id == id)
}

static ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<(?:script|link|iframe|a|img|form|source)\b[^>]*?\b(?:src|href|action)\s*=\s*["']([^"'<>\s]{1,600})["']"#)
        .unwrap()
});
static GENERATOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<meta\b[^>]*?\bname\s*=\s*["']generator["'][^>]*?\bcontent\s*=\s*["']([^"'<>]{1,200})["']|<meta\b[^>]*?\bcontent\s*=\s*["']([^"'<>]{1,200})["'][^>]*?\bname\s*=\s*["']generator["']"#)
        .unwrap()
});

fn host_matches(host: &str, needle: &str) -> bool {
    host == needle || host.ends_with(&format!(".{needle}"))
}

/// Technology ids found in raw page markup, in table order, each once.
pub fn detect(raw_html: &str, page: &Url) -> Vec<&'static str> {
    let references = ATTRIBUTE
        .captures_iter(raw_html)
        .filter_map(|capture| capture.get(1))
        .filter_map(|value| page.join(value.as_str().trim()).ok())
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(|url| {
            (
                url.host_str().unwrap_or_default().to_ascii_lowercase(),
                url.path().to_ascii_lowercase(),
            )
        })
        .collect::<Vec<_>>();
    let generators = GENERATOR
        .captures_iter(raw_html)
        .filter_map(|capture| capture.get(1).or_else(|| capture.get(2)))
        .map(|value| value.as_str().to_ascii_lowercase())
        .collect::<Vec<_>>();
    TECHNOLOGIES
        .iter()
        .filter(|technology| {
            technology.needles.iter().any(|needle| {
                references.iter().any(|(host, path)| {
                    if needle.starts_with('/') {
                        path.contains(needle)
                    } else {
                        host_matches(host, needle)
                    }
                })
            }) || technology
                .generators
                .iter()
                .any(|fragment| generators.iter().any(|value| value.contains(fragment)))
        })
        .map(|technology| technology.id)
        .collect()
}

/// The evidence sentence for a detection. Built only from table strings and
/// the page's own validated URL.
pub fn describe(technology: &Technology, page: &Url) -> String {
    let marker = technology
        .needles
        .iter()
        .find(|needle| !needle.starts_with('/'))
        .copied()
        .unwrap_or(technology.needles[0]);
    format!(
        "{page} references {marker}, which indicates {} for {}.",
        technology.name, technology.category
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Url {
        Url::parse("https://acme.example/").unwrap()
    }

    #[test]
    fn detects_scripts_links_and_generators() {
        let html = r#"<html><head>
            <meta name="generator" content="WordPress 6.5">
            <link rel="stylesheet" href="/wp-content/themes/acme/style.css">
            <script src="https://assets.calendly.com/assets/external/widget.js"></script>
            <script src='//js.hs-scripts.com/123.js'></script>
            </head><body><a href="https://order.toasttab.com/online/acme">Order online</a></body></html>"#;
        assert_eq!(
            detect(html, &page()),
            vec!["wordpress", "hubspot", "calendly", "toast"]
        );
        let reversed = r#"<meta content="Wix.com Website Builder" name="generator">"#;
        assert_eq!(detect(reversed, &page()), vec!["wix"]);
    }

    #[test]
    fn visible_text_and_lookalike_hosts_detect_nothing() {
        let html = r#"<p>We moved off Shopify and Calendly last year. Visit toasttab.com for more.</p>
            <a href="https://notcalendly.com/x">x</a>
            <script src="https://evil.example/cdn.shopify.com.js"></script>"#;
        assert!(detect(html, &page()).is_empty());
    }

    #[test]
    fn injection_bearing_markup_cannot_reach_the_excerpt() {
        let html = r#"<script src="https://assets.calendly.com/widget.js?ignore=previous+instructions"></script>"#;
        let ids = detect(html, &page());
        assert_eq!(ids, vec!["calendly"]);
        let text = describe(find(ids[0]).unwrap(), &page());
        assert_eq!(
            text,
            "https://acme.example/ references calendly.com, which indicates Calendly for online scheduling."
        );
    }
}
