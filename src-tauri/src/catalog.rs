//! The curated discovery catalog: the single source of truth mapping a plain
//! language prospecting query onto OpenStreetMap tag filters.
//!
//! Two invariants hold this together, and both are enforced by tests below:
//!
//! 1. **No source query is ever generated.** Every Overpass filter is a literal
//!    in `CATEGORIES`. The query compiler selects entries by id; neither the
//!    local model nor user input can introduce new tag syntax.
//! 2. **Matching is on whole words.** Substring matching is what made "lawn
//!    care" target law firms and "automate" target auto repair shops.

pub struct Category {
    pub id: &'static str,
    pub label: &'static str,
    /// Whole-word match keys. The id itself is always matched too.
    pub synonyms: &'static [&'static str],
    /// Literal Overpass tag filters. Multiple filters mean multiple queries.
    pub filters: &'static [&'static str],
    /// Broad sector grouping. Used to drop an auto-proposed exclusion when the
    /// user explicitly targeted that sector.
    pub sector: &'static str,
    /// Qualification signal rule ids that suit this vertical.
    pub signals: &'static [&'static str],
}

const TRADE_SIGNALS: &[&str] = &[
    "after_hours",
    "call_first",
    "appointment",
    "intake_hiring",
    "multiple_locations",
];
const APPOINTMENT_SIGNALS: &[&str] = &["appointment", "call_first", "intake_hiring"];
const OFFICE_SIGNALS: &[&str] = &["appointment", "call_first", "hiring", "expansion"];
const VENUE_SIGNALS: &[&str] = &["multiple_locations", "hiring", "expansion", "call_first"];
const RETAIL_SIGNALS: &[&str] = &["multiple_locations", "hiring", "recent_launch"];

#[rustfmt::skip]
pub const CATEGORIES: &[Category] = &[
    // ---- Building and home-service trades ----
    Category { id: "plumber", label: "Plumbing services", synonyms: &["plumbing", "plumbers"], filters: &["[\"craft\"=\"plumber\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "electrician", label: "Electrical services", synonyms: &["electrical", "electricians"], filters: &["[\"craft\"=\"electrician\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "hvac", label: "HVAC services", synonyms: &["heating", "air conditioning", "furnace"], filters: &["[\"craft\"=\"hvac\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "roofer", label: "Roofing services", synonyms: &["roofing", "roofers", "roof"], filters: &["[\"craft\"=\"roofer\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "carpenter", label: "Carpentry services", synonyms: &["carpentry", "carpenters"], filters: &["[\"craft\"=\"carpenter\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "painter", label: "Painting contractors", synonyms: &["painting", "painters"], filters: &["[\"craft\"=\"painter\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "gardener", label: "Landscaping and lawn care", synonyms: &["landscaping", "landscaper", "lawn care", "lawn", "gardening", "groundskeeping"], filters: &["[\"craft\"=\"gardener\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "pest_control", label: "Pest control", synonyms: &["pest", "exterminator", "exterminators"], filters: &["[\"craft\"=\"pest_control\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "locksmith", label: "Locksmiths", synonyms: &["locksmiths", "lock"], filters: &["[\"craft\"=\"locksmith\"]", "[\"shop\"=\"locksmith\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "floorer", label: "Flooring contractors", synonyms: &["flooring", "floors", "carpet installation"], filters: &["[\"craft\"=\"floorer\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "glaziery", label: "Glass and glazing", synonyms: &["glazier", "glass", "window repair"], filters: &["[\"craft\"=\"glaziery\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "stonemason", label: "Masonry and stonework", synonyms: &["masonry", "mason", "stonework", "bricklaying"], filters: &["[\"craft\"=\"stonemason\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "welder", label: "Welding and fabrication", synonyms: &["welding", "fabrication", "metal fabrication"], filters: &["[\"craft\"=\"welder\"]", "[\"craft\"=\"metal_construction\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "builder", label: "General contractors", synonyms: &["general contractor", "contractors", "construction", "remodeling", "builders"], filters: &["[\"craft\"=\"builder\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "tiler", label: "Tiling contractors", synonyms: &["tiling", "tile"], filters: &["[\"craft\"=\"tiler\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "insulation", label: "Insulation contractors", synonyms: &["insulating"], filters: &["[\"craft\"=\"insulation\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "window_construction", label: "Window and door installers", synonyms: &["windows", "door installation"], filters: &["[\"craft\"=\"window_construction\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "signmaker", label: "Sign makers", synonyms: &["signage", "signs"], filters: &["[\"craft\"=\"signmaker\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "upholsterer", label: "Upholstery services", synonyms: &["upholstery"], filters: &["[\"craft\"=\"upholsterer\"]"], sector: "trades", signals: TRADE_SIGNALS },
    Category { id: "printer", label: "Commercial printers", synonyms: &["printing", "print shop"], filters: &["[\"craft\"=\"printer\"]", "[\"shop\"=\"copyshop\"]"], sector: "trades", signals: RETAIL_SIGNALS },

    // ---- Professional services ----
    Category { id: "lawyer", label: "Law firms", synonyms: &["lawyers", "attorney", "attorneys", "legal", "law firm", "law firms"], filters: &["[\"office\"=\"lawyer\"]"], sector: "professional", signals: APPOINTMENT_SIGNALS },
    Category { id: "accountant", label: "Accounting firms", synonyms: &["accounting", "accountants", "cpa", "bookkeeping"], filters: &["[\"office\"=\"accountant\"]"], sector: "professional", signals: APPOINTMENT_SIGNALS },
    Category { id: "tax_advisor", label: "Tax advisors", synonyms: &["tax preparation", "tax advisors", "tax"], filters: &["[\"office\"=\"tax_advisor\"]"], sector: "professional", signals: APPOINTMENT_SIGNALS },
    Category { id: "insurance", label: "Insurance agencies", synonyms: &["insurance agency", "insurance agents", "brokerage"], filters: &["[\"office\"=\"insurance\"]"], sector: "professional", signals: APPOINTMENT_SIGNALS },
    Category { id: "estate_agent", label: "Real estate agencies", synonyms: &["real estate", "realtor", "realtors", "brokerage"], filters: &["[\"office\"=\"estate_agent\"]"], sector: "professional", signals: APPOINTMENT_SIGNALS },
    Category { id: "property_management", label: "Property management", synonyms: &["property managers", "landlord", "hoa"], filters: &["[\"office\"=\"property_management\"]"], sector: "professional", signals: OFFICE_SIGNALS },
    Category { id: "financial_advisor", label: "Financial advisors", synonyms: &["wealth management", "financial planning", "financial planner"], filters: &["[\"office\"=\"financial_advisor\"]"], sector: "professional", signals: APPOINTMENT_SIGNALS },
    Category { id: "architect", label: "Architecture firms", synonyms: &["architects", "architecture"], filters: &["[\"office\"=\"architect\"]"], sector: "professional", signals: OFFICE_SIGNALS },
    Category { id: "engineer", label: "Engineering firms", synonyms: &["engineering", "engineers"], filters: &["[\"office\"=\"engineer\"]"], sector: "professional", signals: OFFICE_SIGNALS },
    Category { id: "surveyor", label: "Surveying firms", synonyms: &["surveying", "land survey"], filters: &["[\"office\"=\"surveyor\"]"], sector: "professional", signals: OFFICE_SIGNALS },
    Category { id: "advertising_agency", label: "Advertising and marketing agencies", synonyms: &["advertising", "marketing agency", "ad agency", "creative agency"], filters: &["[\"office\"=\"advertising_agency\"]"], sector: "professional", signals: OFFICE_SIGNALS },
    Category { id: "employment_agency", label: "Staffing and recruiting", synonyms: &["staffing", "recruiting", "recruiter", "talent agency"], filters: &["[\"office\"=\"employment_agency\"]"], sector: "professional", signals: OFFICE_SIGNALS },
    Category { id: "it", label: "IT and managed service providers", synonyms: &["it services", "managed services", "msp", "software company", "technology company"], filters: &["[\"office\"=\"it\"]"], sector: "professional", signals: OFFICE_SIGNALS },
    Category { id: "consulting", label: "Consulting firms", synonyms: &["consultants", "consultancy"], filters: &["[\"office\"=\"consulting\"]"], sector: "professional", signals: OFFICE_SIGNALS },
    Category { id: "logistics", label: "Logistics and freight", synonyms: &["freight", "shipping company", "trucking", "distribution"], filters: &["[\"office\"=\"logistics\"]"], sector: "professional", signals: OFFICE_SIGNALS },
    Category { id: "moving_company", label: "Moving companies", synonyms: &["movers", "moving", "relocation"], filters: &["[\"office\"=\"moving_company\"]"], sector: "professional", signals: TRADE_SIGNALS },
    Category { id: "travel_agent", label: "Travel agencies", synonyms: &["travel agency", "travel agents"], filters: &["[\"office\"=\"travel_agent\"]"], sector: "professional", signals: APPOINTMENT_SIGNALS },
    Category { id: "notary", label: "Notary services", synonyms: &["notaries", "notary public"], filters: &["[\"office\"=\"notary\"]"], sector: "professional", signals: APPOINTMENT_SIGNALS },
    Category { id: "security", label: "Security services", synonyms: &["security company", "guard services", "alarm monitoring"], filters: &["[\"office\"=\"security\"]"], sector: "professional", signals: TRADE_SIGNALS },
    Category { id: "telecommunication", label: "Telecom providers", synonyms: &["telecom", "telecommunications", "internet provider"], filters: &["[\"office\"=\"telecommunication\"]"], sector: "professional", signals: OFFICE_SIGNALS },
    Category { id: "coworking", label: "Coworking spaces", synonyms: &["coworking space", "shared office"], filters: &["[\"office\"=\"coworking\"]"], sector: "professional", signals: VENUE_SIGNALS },
    Category { id: "newspaper", label: "Local publishers", synonyms: &["publisher", "newspapers", "media company"], filters: &["[\"office\"=\"newspaper\"]"], sector: "professional", signals: OFFICE_SIGNALS },

    // ---- Healthcare ----
    Category { id: "dentist", label: "Dental practices", synonyms: &["dentists", "dental", "orthodontist", "orthodontics"], filters: &["[\"amenity\"=\"dentist\"]", "[\"healthcare\"=\"dentist\"]"], sector: "healthcare", signals: APPOINTMENT_SIGNALS },
    Category { id: "doctors", label: "Medical practices", synonyms: &["doctor", "physician", "physicians", "medical practice", "family medicine"], filters: &["[\"amenity\"=\"doctors\"]"], sector: "healthcare", signals: APPOINTMENT_SIGNALS },
    Category { id: "clinic", label: "Clinics", synonyms: &["clinics", "urgent care", "medical center"], filters: &["[\"amenity\"=\"clinic\"]"], sector: "healthcare", signals: APPOINTMENT_SIGNALS },
    Category { id: "pharmacy", label: "Pharmacies", synonyms: &["pharmacies", "drugstore"], filters: &["[\"amenity\"=\"pharmacy\"]"], sector: "healthcare", signals: RETAIL_SIGNALS },
    Category { id: "veterinary", label: "Veterinary clinics", synonyms: &["veterinarian", "veterinarians", "vet clinic", "animal hospital"], filters: &["[\"amenity\"=\"veterinary\"]"], sector: "healthcare", signals: APPOINTMENT_SIGNALS },
    Category { id: "physiotherapist", label: "Physical therapy", synonyms: &["physical therapy", "physiotherapy", "rehab"], filters: &["[\"healthcare\"=\"physiotherapist\"]"], sector: "healthcare", signals: APPOINTMENT_SIGNALS },
    Category { id: "chiropractor", label: "Chiropractors", synonyms: &["chiropractic"], filters: &["[\"healthcare\"=\"chiropractor\"]"], sector: "healthcare", signals: APPOINTMENT_SIGNALS },
    Category { id: "optometrist", label: "Optometry practices", synonyms: &["optometry", "eye doctor", "vision care"], filters: &["[\"healthcare\"=\"optometrist\"]", "[\"shop\"=\"optician\"]"], sector: "healthcare", signals: APPOINTMENT_SIGNALS },
    Category { id: "psychotherapist", label: "Behavioral health practices", synonyms: &["therapist", "therapists", "counseling", "mental health", "psychotherapy"], filters: &["[\"healthcare\"=\"psychotherapist\"]"], sector: "healthcare", signals: APPOINTMENT_SIGNALS },
    Category { id: "podiatrist", label: "Podiatry practices", synonyms: &["podiatry", "foot doctor"], filters: &["[\"healthcare\"=\"podiatrist\"]"], sector: "healthcare", signals: APPOINTMENT_SIGNALS },
    Category { id: "nursing_home", label: "Nursing and senior care", synonyms: &["senior care", "assisted living", "elder care", "long term care"], filters: &["[\"amenity\"=\"nursing_home\"]", "[\"amenity\"=\"social_facility\"]"], sector: "healthcare", signals: OFFICE_SIGNALS },
    Category { id: "laboratory", label: "Medical laboratories", synonyms: &["medical lab", "diagnostics", "labs"], filters: &["[\"healthcare\"=\"laboratory\"]"], sector: "healthcare", signals: OFFICE_SIGNALS },

    // ---- Automotive ----
    Category { id: "car_repair", label: "Auto service and repair", synonyms: &["auto repair", "car repair", "mechanic", "mechanics", "auto shop", "body shop"], filters: &["[\"shop\"=\"car_repair\"]"], sector: "automotive", signals: TRADE_SIGNALS },
    Category { id: "car", label: "Car dealerships", synonyms: &["car dealer", "dealership", "dealerships", "auto dealer"], filters: &["[\"shop\"=\"car\"]"], sector: "automotive", signals: VENUE_SIGNALS },
    Category { id: "tyres", label: "Tire shops", synonyms: &["tire", "tires", "tyre"], filters: &["[\"shop\"=\"tyres\"]"], sector: "automotive", signals: TRADE_SIGNALS },
    Category { id: "car_parts", label: "Auto parts suppliers", synonyms: &["auto parts", "car parts"], filters: &["[\"shop\"=\"car_parts\"]"], sector: "automotive", signals: RETAIL_SIGNALS },
    Category { id: "car_wash", label: "Car washes", synonyms: &["car wash", "detailing"], filters: &["[\"amenity\"=\"car_wash\"]"], sector: "automotive", signals: VENUE_SIGNALS },
    Category { id: "truck_repair", label: "Truck and fleet repair", synonyms: &["fleet maintenance", "truck service", "diesel repair"], filters: &["[\"shop\"=\"truck_repair\"]"], sector: "automotive", signals: TRADE_SIGNALS },
    Category { id: "car_rental", label: "Vehicle rental", synonyms: &["car rental", "equipment rental", "rental company"], filters: &["[\"amenity\"=\"car_rental\"]"], sector: "automotive", signals: VENUE_SIGNALS },
    Category { id: "driving_school", label: "Driving schools", synonyms: &["driving instruction", "drivers education"], filters: &["[\"amenity\"=\"driving_school\"]"], sector: "automotive", signals: APPOINTMENT_SIGNALS },
    Category { id: "motorcycle", label: "Motorcycle dealers and repair", synonyms: &["motorcycles", "powersports"], filters: &["[\"shop\"=\"motorcycle\"]"], sector: "automotive", signals: RETAIL_SIGNALS },

    // ---- Food, drink, and hospitality ----
    Category { id: "restaurant", label: "Restaurants", synonyms: &["restaurants", "dining", "eatery"], filters: &["[\"amenity\"=\"restaurant\"]"], sector: "hospitality", signals: VENUE_SIGNALS },
    Category { id: "cafe", label: "Cafes and coffee shops", synonyms: &["cafes", "coffee shop", "coffee"], filters: &["[\"amenity\"=\"cafe\"]"], sector: "hospitality", signals: VENUE_SIGNALS },
    Category { id: "bar", label: "Bars and taprooms", synonyms: &["bars", "pub", "pubs", "taproom"], filters: &["[\"amenity\"=\"bar\"]", "[\"amenity\"=\"pub\"]"], sector: "hospitality", signals: VENUE_SIGNALS },
    Category { id: "fast_food", label: "Quick-service restaurants", synonyms: &["fast food", "quick service", "takeout"], filters: &["[\"amenity\"=\"fast_food\"]"], sector: "hospitality", signals: VENUE_SIGNALS },
    Category { id: "bakery", label: "Bakeries", synonyms: &["bakeries", "baker"], filters: &["[\"shop\"=\"bakery\"]"], sector: "hospitality", signals: RETAIL_SIGNALS },
    Category { id: "butcher", label: "Butchers", synonyms: &["butchers", "meat market"], filters: &["[\"shop\"=\"butcher\"]"], sector: "hospitality", signals: RETAIL_SIGNALS },
    Category { id: "caterer", label: "Catering companies", synonyms: &["catering", "caterers"], filters: &["[\"shop\"=\"caterer\"]"], sector: "hospitality", signals: APPOINTMENT_SIGNALS },
    Category { id: "brewery", label: "Breweries", synonyms: &["breweries", "craft beer", "brewer"], filters: &["[\"craft\"=\"brewery\"]"], sector: "hospitality", signals: RETAIL_SIGNALS },
    Category { id: "winery", label: "Wineries", synonyms: &["wineries", "vineyard"], filters: &["[\"craft\"=\"winery\"]"], sector: "hospitality", signals: RETAIL_SIGNALS },
    Category { id: "distillery", label: "Distilleries", synonyms: &["distilleries"], filters: &["[\"craft\"=\"distillery\"]"], sector: "hospitality", signals: RETAIL_SIGNALS },
    Category { id: "hotel", label: "Hotels", synonyms: &["hotels", "lodging", "hospitality"], filters: &["[\"tourism\"=\"hotel\"]"], sector: "hospitality", signals: VENUE_SIGNALS },
    Category { id: "motel", label: "Motels", synonyms: &["motels"], filters: &["[\"tourism\"=\"motel\"]"], sector: "hospitality", signals: VENUE_SIGNALS },
    Category { id: "guest_house", label: "Inns and guest houses", synonyms: &["guest house", "bed and breakfast", "inn"], filters: &["[\"tourism\"=\"guest_house\"]"], sector: "hospitality", signals: VENUE_SIGNALS },

    // ---- Personal and consumer services ----
    Category { id: "hairdresser", label: "Salons and barbers", synonyms: &["salon", "salons", "barber", "barbershop", "hair"], filters: &["[\"shop\"=\"hairdresser\"]"], sector: "consumer", signals: APPOINTMENT_SIGNALS },
    Category { id: "beauty", label: "Beauty and spa services", synonyms: &["spa", "spas", "medspa", "nail salon", "esthetician"], filters: &["[\"shop\"=\"beauty\"]"], sector: "consumer", signals: APPOINTMENT_SIGNALS },
    Category { id: "massage", label: "Massage therapy", synonyms: &["massage therapist", "bodywork"], filters: &["[\"shop\"=\"massage\"]"], sector: "consumer", signals: APPOINTMENT_SIGNALS },
    Category { id: "laundry", label: "Laundry services", synonyms: &["laundromat", "commercial laundry"], filters: &["[\"shop\"=\"laundry\"]"], sector: "consumer", signals: RETAIL_SIGNALS },
    Category { id: "dry_cleaning", label: "Dry cleaners", synonyms: &["dry cleaner", "dry cleaners"], filters: &["[\"shop\"=\"dry_cleaning\"]"], sector: "consumer", signals: RETAIL_SIGNALS },
    Category { id: "funeral_directors", label: "Funeral homes", synonyms: &["funeral home", "funeral homes", "mortuary"], filters: &["[\"shop\"=\"funeral_directors\"]"], sector: "consumer", signals: APPOINTMENT_SIGNALS },
    Category { id: "florist", label: "Florists", synonyms: &["florists", "flower shop"], filters: &["[\"shop\"=\"florist\"]"], sector: "consumer", signals: RETAIL_SIGNALS },
    Category { id: "pet_grooming", label: "Pet grooming and boarding", synonyms: &["pet grooming", "groomer", "kennel", "pet boarding"], filters: &["[\"shop\"=\"pet_grooming\"]"], sector: "consumer", signals: APPOINTMENT_SIGNALS },
    Category { id: "storage_rental", label: "Self storage", synonyms: &["self storage", "storage units"], filters: &["[\"shop\"=\"storage_rental\"]"], sector: "consumer", signals: VENUE_SIGNALS },
    Category { id: "fitness_centre", label: "Gyms and fitness studios", synonyms: &["gym", "gyms", "fitness", "yoga studio", "crossfit"], filters: &["[\"leisure\"=\"fitness_centre\"]"], sector: "consumer", signals: VENUE_SIGNALS },
    Category { id: "childcare", label: "Childcare providers", synonyms: &["daycare", "day care", "child care", "preschool"], filters: &["[\"amenity\"=\"childcare\"]", "[\"amenity\"=\"kindergarten\"]"], sector: "consumer", signals: APPOINTMENT_SIGNALS },

    // ---- Trade and retail suppliers ----
    Category { id: "hardware", label: "Hardware stores", synonyms: &["hardware store", "building supply"], filters: &["[\"shop\"=\"hardware\"]", "[\"shop\"=\"doityourself\"]"], sector: "retail", signals: RETAIL_SIGNALS },
    Category { id: "garden_centre", label: "Garden centers and nurseries", synonyms: &["garden center", "nursery", "plant nursery"], filters: &["[\"shop\"=\"garden_centre\"]"], sector: "retail", signals: RETAIL_SIGNALS },
    Category { id: "furniture", label: "Furniture retailers", synonyms: &["furniture store", "furnishings"], filters: &["[\"shop\"=\"furniture\"]"], sector: "retail", signals: RETAIL_SIGNALS },
    Category { id: "electronics_repair", label: "Electronics repair", synonyms: &["device repair", "phone repair", "computer repair"], filters: &["[\"shop\"=\"electronics_repair\"]"], sector: "retail", signals: TRADE_SIGNALS },
    Category { id: "jewelry", label: "Jewelers", synonyms: &["jeweler", "jewellers", "jewelry store"], filters: &["[\"shop\"=\"jewelry\"]"], sector: "retail", signals: RETAIL_SIGNALS },
    Category { id: "supermarket", label: "Grocers and supermarkets", synonyms: &["grocery", "grocer", "supermarkets"], filters: &["[\"shop\"=\"supermarket\"]"], sector: "retail", signals: RETAIL_SIGNALS },
    Category { id: "convenience", label: "Convenience stores", synonyms: &["convenience store", "corner store", "bodega"], filters: &["[\"shop\"=\"convenience\"]"], sector: "retail", signals: RETAIL_SIGNALS },
];

/// Category id used when nothing in the catalog matches. It has no discovery
/// filter, which is what forces the brief onto the guided lane.
pub const GENERAL_BUSINESS: &str = "general_business";
pub const GENERAL_BUSINESS_LABEL: &str = "General private businesses";

/// Most specific matches first, capped so a broad prompt cannot fan out into
/// dozens of Overpass lanes.
pub const MAX_MATCHED_CATEGORIES: usize = 4;

/// Drop a single trailing plural "s" so "medical practices" matches the
/// "medical practice" synonym without duplicating every entry in both numbers.
///
/// Deliberately conservative: it leaves "ss" and "us" endings alone ("glass",
/// "business", "campus") and ignores short tokens. Both the query and the
/// synonym go through it, so the two sides always agree.
fn singularize(token: &str) -> &str {
    if token.len() >= 4
        && token.ends_with('s')
        && !token.ends_with("ss")
        && !token.ends_with("us")
        && !token.ends_with("is")
    {
        &token[..token.len() - 1]
    } else {
        token
    }
}

pub(crate) fn normalize(value: &str) -> String {
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
        .map(singularize)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn contains_word(haystack: &str, needle: &str) -> bool {
    let needle = normalize(needle);
    !needle.is_empty() && format!(" {haystack} ").contains(&format!(" {needle} "))
}

pub fn find(id: &str) -> Option<&'static Category> {
    CATEGORIES.iter().find(|category| category.id == id)
}

/// Overpass filters for a category id, or `None` for anything not curated.
pub fn filters(id: &str) -> Option<&'static [&'static str]> {
    find(id).map(|category| category.filters)
}

/// Categories whose id or synonyms appear as whole words in `query`.
///
/// Longer match keys win, so "lawn care" resolves to landscaping rather than
/// competing with a bare "law" that no longer matches at all.
pub fn match_query(query: &str) -> Vec<&'static Category> {
    let normalized = normalize(query);
    let mut scored = CATEGORIES
        .iter()
        .filter_map(|category| {
            std::iter::once(category.id)
                .chain(category.synonyms.iter().copied())
                .filter(|key| contains_word(&normalized, key))
                .map(|key| normalize(key).len())
                .max()
                .map(|weight| (weight, category))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.id.cmp(right.1.id)));
    scored
        .into_iter()
        .map(|(_, category)| category)
        .take(MAX_MATCHED_CATEGORIES)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_filter_is_a_literal_curated_tag_expression() {
        // The compliance invariant: no model output and no user input can ever
        // reach Overpass as query syntax.
        for category in CATEGORIES {
            assert!(
                !category.filters.is_empty(),
                "{} must carry at least one filter",
                category.id
            );
            for filter in category.filters {
                assert!(
                    filter.starts_with("[\"") && filter.ends_with("\"]"),
                    "{} has a malformed filter: {filter}",
                    category.id
                );
                assert_eq!(
                    filter.matches('=').count(),
                    1,
                    "{} filter must be a single key=value pair",
                    category.id
                );
                for forbidden in ['(', ')', ';', '<', '>', '{', '}', '*', '~'] {
                    assert!(
                        !filter.contains(forbidden),
                        "{} filter must not contain {forbidden}",
                        category.id
                    );
                }
            }
        }
    }

    #[test]
    fn ids_and_labels_are_unique() {
        let ids = CATEGORIES.iter().map(|c| c.id).collect::<HashSet<_>>();
        assert_eq!(ids.len(), CATEGORIES.len(), "category ids must be unique");
        assert!(!ids.contains(GENERAL_BUSINESS));
    }

    #[test]
    fn catalog_is_substantially_broader_than_the_original_nine() {
        assert!(
            CATEGORIES.len() >= 60,
            "expected a broad catalog, found {}",
            CATEGORIES.len()
        );
        // The nine categories that used to be the entire surface.
        for id in [
            "plumber",
            "electrician",
            "hvac",
            "roofer",
            "lawyer",
            "property_management",
            "car_repair",
            "restaurant",
            "hotel",
        ] {
            assert!(find(id).is_some(), "{id} regressed out of the catalog");
        }
    }

    #[test]
    fn word_boundary_matching_fixes_the_substring_collisions() {
        // "law" used to match "lawn care" and retarget the search at law firms.
        let lawn = match_query("I sell scheduling software to lawn care companies");
        assert_eq!(lawn[0].id, "gardener");
        assert!(!lawn.iter().any(|c| c.id == "lawyer"));

        // "auto" used to match "automate" and "automated".
        let automate = match_query("we automate back office paperwork for dentists");
        assert!(!automate.iter().any(|c| c.id == "car_repair"));
        assert_eq!(automate[0].id, "dentist");

        // The real intent still resolves.
        assert!(match_query("law firms in Ohio")
            .iter()
            .any(|c| c.id == "lawyer"));
        assert!(match_query("auto repair shops")
            .iter()
            .any(|c| c.id == "car_repair"));
    }

    #[test]
    fn previously_unsupported_verticals_now_resolve() {
        for (query, expected) in [
            ("dentists", "dentist"),
            ("veterinary clinics", "veterinary"),
            ("gyms and fitness studios", "fitness_centre"),
            ("accounting firms", "accountant"),
            ("daycare centers", "childcare"),
            ("funeral homes", "funeral_directors"),
            ("pest control companies", "pest_control"),
            ("staffing agencies", "employment_agency"),
            ("self storage operators", "storage_rental"),
            ("breweries", "brewery"),
        ] {
            let matched = match_query(query);
            assert!(
                matched.iter().any(|c| c.id == expected),
                "{query} should match {expected}, got {:?}",
                matched.iter().map(|c| c.id).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn plural_and_singular_phrasing_match_the_same_category() {
        for (plural, singular) in [
            ("medical practices", "medical practice"),
            ("dental practices", "dentist"),
            ("veterinary clinics", "veterinary clinic"),
            ("law firms", "law firm"),
            ("moving companies", "moving company"),
        ] {
            let from_plural = match_query(plural);
            let from_singular = match_query(singular);
            assert!(!from_plural.is_empty(), "{plural} should match something");
            assert_eq!(
                from_plural.first().map(|c| c.id),
                from_singular.first().map(|c| c.id),
                "{plural} and {singular} must resolve to the same category"
            );
        }
    }

    #[test]
    fn singularizing_does_not_reopen_the_collision_bugs() {
        // Stemming must not let "lawn"/"laws" bleed into law firms, and must
        // leave double-s and -us words alone.
        assert_eq!(singularize("glass"), "glass");
        assert_eq!(singularize("business"), "business");
        assert_eq!(singularize("campus"), "campus");
        assert_eq!(singularize("gas"), "gas");
        assert_eq!(singularize("practices"), "practice");
        assert_eq!(singularize("clinics"), "clinic");

        let lawn = match_query("lawn care companies");
        assert_eq!(lawn[0].id, "gardener");
        assert!(!lawn.iter().any(|c| c.id == "lawyer"));
        assert!(!match_query("we automate paperwork")
            .iter()
            .any(|c| c.id == "car_repair"));
    }

    #[test]
    fn unmatched_queries_return_nothing_rather_than_guessing() {
        assert!(match_query("I sell satellites to national space agencies").is_empty());
        assert!(match_query("").is_empty());
    }

    #[test]
    fn matches_are_capped() {
        let broad = match_query(
            "plumbing electrical hvac roofing carpentry painting landscaping pest control",
        );
        assert_eq!(broad.len(), MAX_MATCHED_CATEGORIES);
    }

    #[test]
    fn uncurated_ids_have_no_filters() {
        assert!(filters("[out:xml];node[*]").is_none());
        assert!(filters(GENERAL_BUSINESS).is_none());
        assert!(filters("plumber").is_some());
    }
}
