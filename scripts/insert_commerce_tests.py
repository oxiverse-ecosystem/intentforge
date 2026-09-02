path = r"C:\Users\Likhith\Documents\Projects\intentforge\services\gateway\src\main.rs"
with open(path, "rb") as f:
    src = f.read()

# Read the anchor from the file itself to avoid encoding issues
anchor_start = b"    fn product_page_has_null_event_fields() {"
anchor_end = b"        assert_eq!(d.name.as_deref(), Some(\"Widget\"));\r\n    }\r\n}"

start_idx = src.find(anchor_start)
end_idx = src.find(anchor_end) + len(anchor_end)
assert start_idx > 0 and end_idx > start_idx, "anchor not found"

# Replacement: keep anchor, append new tests after anchor_end
replacement = src[start_idx:end_idx]

new_tests = b"""

    // -- JobPosting fixtures + tests --

    const HTML_JOBPOSTING_FULL: &str = r#"<!doctype html><html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org/",
  "@type": "JobPosting",
  "title": "Senior Rust Engineer",
  "datePosted": "2026-08-15",
  "hiringOrganization": {
    "@type": "Organization",
    "name": "Acme Corp"
  },
  "jobLocation": {
    "@type": "Place",
    "name": "Remote",
    "address": "Anywhere, Earth"
  },
  "baseSalary": {
    "@type": "MonetaryAmount",
    "value": 150000.0,
    "currency": "USD"
  },
  "employmentType": "FULL_TIME"
}
</script></head><body></body></html>"#;

    const HTML_JOBPOSTING_MICRODATA: &str = r#"<!doctype html><html><body>
<div itemscope itemtype="https://schema.org/JobPosting">
  <span itemprop="title">Frontend Developer</span>
  <span itemprop="hiringOrganization">Tech Inc</span>
  <span itemprop="joblocation">San Francisco, CA</span>
</div>
</body></html>"#;

    const HTML_COURSE_FULL: &str = r#"<!doctype html><html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org/",
  "@type": "Course",
  "name": "Rust Fundamentals",
  "provider": {
    "@type": "Organization",
    "name": "Rust Academy"
  },
  "hasCourseInstance": {
    "@type": "CourseInstance",
    "courseMode": "online"
  }
}
</script></head><body></body></html>"#;

    const HTML_RECIPE_FULL: &str = r#"<!doctype html><html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org/",
  "@type": "Recipe",
  "name": "Classic Spaghetti Carbonara",
  "prepTime": "PT15M",
  "cookTime": "PT20M",
  "totalTime": "PT35M",
  "recipeYield": "4 servings"
}
</script></head><body></body></html>"#;

    const HTML_PRODUCT_NO_NEW_TYPES: &str = r#"<!doctype html><html><head>
<script type="application/ld+json">
{
  "@context": "https://schema.org/",
  "@type": "Product",
  "name": "Widget",
  "offers": {"@type": "Offer", "price": "9.99", "priceCurrency": "USD"}
}
</script></head><body></body></html>"#;

    #[test]
    fn extracts_jobposting_fields_from_jsonld() {
        let o = extract_commerce_offer(HTML_JOBPOSTING_FULL, "https://jobs.example.com/rust-eng");
        let d = o.data.as_ref().unwrap();
        assert_eq!(d.job_title.as_deref(), Some("Senior Rust Engineer"));
        assert_eq!(d.job_posted_at.as_deref(), Some("2026-08-15"));
        assert_eq!(d.hiring_organization.as_deref(), Some("Acme Corp"));
        assert_eq!(d.job_location.as_deref(), Some("Remote"), "job_location prefers Place.name");
        assert_eq!(d.base_salary.as_deref(), Some("150000 USD"));
        assert_eq!(d.employment_type.as_deref(), Some("FULL_TIME"));
        assert_eq!(o.source.as_deref(), Some("json-ld"));
        assert_eq!(d.price, None, "job has no product price");
    }

    #[test]
    fn extracts_jobposting_from_microdata() {
        let o = extract_commerce_offer(HTML_JOBPOSTING_MICRODATA, "https://jobs.example.com/frontend");
        let d = o.data.as_ref().unwrap();
        assert_eq!(d.job_title.as_deref(), Some("Frontend Developer"));
        assert_eq!(d.hiring_organization.as_deref(), Some("Tech Inc"));
        assert_eq!(d.job_location.as_deref(), Some("San Francisco, CA"));
        assert_eq!(o.source.as_deref(), Some("microdata"));
    }

    #[test]
    fn extracts_course_fields_from_jsonld() {
        let o = extract_commerce_offer(HTML_COURSE_FULL, "https://learn.example.com/rust");
        let d = o.data.as_ref().unwrap();
        assert_eq!(d.course_provider.as_deref(), Some("Rust Academy"));
        assert_eq!(d.course_mode.as_deref(), Some("online"));
        assert_eq!(o.source.as_deref(), Some("json-ld"));
        assert_eq!(d.price, None, "course has no product price");
    }

    #[test]
    fn extracts_recipe_fields_from_jsonld() {
        let o = extract_commerce_offer(HTML_RECIPE_FULL, "https://recipes.example.com/carbonara");
        let d = o.data.as_ref().unwrap();
        assert_eq!(d.prep_time.as_deref(), Some("PT15M"));
        assert_eq!(d.cook_time.as_deref(), Some("PT20M"));
        assert_eq!(d.total_time.as_deref(), Some("PT35M"));
        assert_eq!(d.recipe_yield.as_deref(), Some("4 servings"));
        assert_eq!(o.source.as_deref(), Some("json-ld"));
    }

    #[test]
    fn product_page_has_null_new_type_fields() {
        let o = extract_commerce_offer(HTML_PRODUCT_NO_NEW_TYPES, "https://shop.example.com/widget");
        let d = o.data.as_ref().unwrap();
        assert_eq!(d.job_title, None, "job_title null for Product");
        assert_eq!(d.job_posted_at, None, "job_posted_at null for Product");
        assert_eq!(d.hiring_organization, None, "hiring_organization null for Product");
        assert_eq!(d.job_location, None, "job_location null for Product");
        assert_eq!(d.base_salary, None, "base_salary null for Product");
        assert_eq!(d.employment_type, None, "employment_type null for Product");
        assert_eq!(d.course_provider, None, "course_provider null for Product");
        assert_eq!(d.course_mode, None, "course_mode null for Product");
        assert_eq!(d.prep_time, None, "prep_time null for Product");
        assert_eq!(d.cook_time, None, "cook_time null for Product");
        assert_eq!(d.total_time, None, "total_time null for Product");
        assert_eq!(d.recipe_yield, None, "recipe_yield null for Product");
        // Sanity: product fields still extract.
        assert_eq!(d.price, Some(9.99));
        assert_eq!(d.name.as_deref(), Some("Widget"));
    }

    #[test]
    fn jobposting_does_not_populate_event_fields() {
        let o = extract_commerce_offer(HTML_JOBPOSTING_FULL, "https://jobs.example.com/rust-eng");
        let d = o.data.as_ref().unwrap();
        assert_eq!(d.event_start, None, "event_start null for JobPosting");
        assert_eq!(d.event_location, None, "event_location null for JobPosting");
    }
"""

replacement = replacement + new_tests

src = src[:start_idx] + replacement + src[end_idx:]

with open(path, "wb") as f:
    f.write(src)

print("6 new tests inserted")
