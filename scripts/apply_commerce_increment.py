#!/usr/bin/env python3
"""Apply commerce extraction increment: JobPosting, Course/Recipe (CRLF-safe)."""
path = r"C:\Users\Likhith\Documents\Projects\intentforge\services\gateway\src\main.rs"

with open(path, "rb") as f:
    src = f.read()

CRLF = b"\r\n"
LF = b"\n"

def rep(b_old, b_new):
    global src
    assert b_old in src, f"anchor not found: {b_old[:80]!r}..."
    src = src.replace(b_old, b_new, 1)

# All byte strings use explicit \r\n

# 1) walk_commerce_nodes: add jobposting/course/recipe
rep(
    b'                || tl.contains("event")\r\n                {\r\n                    out.push(v.clone());\r\n                }',
    b'                || tl.contains("event")\r\n                    || tl.contains("jobposting")\r\n                    || tl.contains("course")\r\n                    || tl.contains("recipe")\r\n                {\r\n                    out.push(v.clone());\r\n                }'
)

# 2) parse_microdata scope classifier
rep(
    b'                || itype.contains("event")\r\n            {\r\n                Scope::Product',
    b'                || itype.contains("event")\r\n                || itype.contains("jobposting")\r\n                || itype.contains("course")\r\n                || itype.contains("recipe")\r\n            {\r\n                Scope::Product'
)

# 3) merge_jsonld_nodes: insert after age_group block, before the for-loop close `}`
rep(
    b'        if facts.age_group.is_none() {\r\n            facts.age_group = n\r\n                .get("ageGroup")\r\n                .and_then(|v| v.as_str())\r\n                .map(|s| s.to_string());\r\n        }\r\n    }',
    b"""        if facts.age_group.is_none() {
            facts.age_group = n
                .get("ageGroup")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
        // ROADMAP item 1 (increment): extract JobPosting facts
        if facts.job_title.is_none() {
            facts.job_title = n.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
        if facts.job_posted_at.is_none() {
            facts.job_posted_at = n.get("datePosted").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
        if facts.hiring_organization.is_none() {
            facts.hiring_organization = n.get("hiringOrganization").and_then(|org| {
                if let Some(s) = org.as_str() { return Some(s.to_string()); }
                if let Some(obj) = org.as_object() {
                    return obj.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
                }
                None
            });
        }
        if facts.job_location.is_none() {
            facts.job_location = n.get("jobLocation").and_then(|loc| {
                if let Some(s) = loc.as_str() { return Some(s.to_string()); }
                if let Some(obj) = loc.as_object() {
                    if let Some(name) = obj.get("name").and_then(|v| v.as_str()) { return Some(name.to_string()); }
                    if let Some(addr) = obj.get("address").and_then(|v| v.as_str()) { return Some(addr.to_string()); }
                }
                if let Some(arr) = loc.as_array() {
                    return arr.first().and_then(|x| {
                        if let Some(s) = x.as_str() { return Some(s.to_string()); }
                        if let Some(obj) = x.as_object() {
                            if let Some(name) = obj.get("name").and_then(|v| v.as_str()) { return Some(name.to_string()); }
                        }
                        None
                    });
                }
                None
            });
        }
        if facts.base_salary.is_none() {
            facts.base_salary = n.get("baseSalary").and_then(|sal| {
                if let Some(s) = sal.as_str() { return Some(s.to_string()); }
                if let Some(obj) = sal.as_object() {
                    if let Some(val) = obj.get("value") {
                        if let Some(v) = val.as_f64() { let cur = obj.get("currency").and_then(|c| c.as_str()).unwrap_or("USD"); return Some(format!("{} {}", v, cur)); }
                        if let Some(s) = val.as_str() { return Some(s.to_string()); }
                    }
                    if let Some(min) = obj.get("minValue").and_then(|v| v.as_f64()) {
                        if let Some(max) = obj.get("maxValue").and_then(|v| v.as_f64()) { let cur = obj.get("currency").and_then(|c| c.as_str()).unwrap_or("USD"); return Some(format!("{} - {} {}", min, max, cur)); }
                    }
                }
                None
            });
        }
        if facts.employment_type.is_none() {
            facts.employment_type = n.get("employmentType").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
        if facts.course_provider.is_none() {
            facts.course_provider = n.get("provider").and_then(|p| {
                if let Some(s) = p.as_str() { return Some(s.to_string()); }
                if let Some(obj) = p.as_object() { return obj.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()); }
                None
            });
        }
        if facts.course_mode.is_none() {
            facts.course_mode = n.get("hasCourseInstance").and_then(|ci| {
                if let Some(s) = ci.as_str() { return Some(s.to_string()); }
                if let Some(obj) = ci.as_object() { return obj.get("courseMode").and_then(|v| v.as_str()).map(|s| s.to_string()); }
                if let Some(arr) = ci.as_array() {
                    return arr.first().and_then(|x| {
                        if let Some(s) = x.as_str() { return Some(s.to_string()); }
                        if let Some(obj) = x.as_object() { return obj.get("courseMode").and_then(|v| v.as_str()).map(|s| s.to_string()); }
                        None
                    });
                }
                None
            });
        }
        if facts.prep_time.is_none() {
            facts.prep_time = n.get("prepTime").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
        if facts.cook_time.is_none() {
            facts.cook_time = n.get("cookTime").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
        if facts.total_time.is_none() {
            facts.total_time = n.get("totalTime").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
        if facts.recipe_yield.is_none() {
            facts.recipe_yield = n.get("recipeYield").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
    }""".replace(b"\n", b"\r\n")
)

# 4) parse_microdata: extend the match block
rep(
    b'            "gtin13" | "gtin14" | "gtin8" | "gtin" | "mpn" => {\r\n                if facts.gtin.is_none() && !content.is_empty() {\r\n                    facts.gtin = Some(content);\r\n                }\r\n            }',
    b"""            "gtin13" | "gtin14" | "gtin8" | "gtin" | "mpn" => {
                if facts.gtin.is_none() && !content.is_empty() {
                    facts.gtin = Some(content);
                }
            }
            "title" => set_str(&mut facts.job_title, content),
            "dateposted" => set_str(&mut facts.job_posted_at, content),
            "hiringorganization" => set_str(&mut facts.hiring_organization, content),
            "joblocation" => set_str(&mut facts.job_location, content),
            "basesalary" => set_str(&mut facts.base_salary, content),
            "employmenttype" => set_str(&mut facts.employment_type, content),
            "provider" => set_str(&mut facts.course_provider, content),
            "coursemode" => set_str(&mut facts.course_mode, content),
            "preptime" => set_str(&mut facts.prep_time, content),
            "cooktime" => set_str(&mut facts.cook_time, content),
            "totaltime" => set_str(&mut facts.total_time, content),
            "recipeyield" => set_str(&mut facts.recipe_yield, content),""".replace(b"\n", b"\r\n")
)

with open(path, "wb") as f:
    f.write(src)

print("All 4 code patches applied")
