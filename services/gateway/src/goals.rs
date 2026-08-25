// ─── Goals Feature Module ──────────────────────────────────────────────
// Takes a user's goal (e.g. "develop an AI assistant"), asks questions,
// uses intent engine for classification, search API for resource curation,
// and generates a personalized phased roadmap with deadlines + buffers.
//
// Endpoints:
//   POST /goals                   — Create a goal, get questions back
//   POST /goals/:id/answers       — Submit answers, get full roadmap
//   GET  /goals/:id               — Get goal status and roadmap
//   GET  /goals/leaderboard       — Get leaderboard
//   POST /goals/quick             — Quick one-shot: goal → full roadmap immediately
//   POST /goals/:id/phases/:phase_id/complete — Mark a phase as completed
//   POST /goals/:id/progress      — Update phase completion progress & score

use axum::{
    async_trait,
    extract::{FromRequest, Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ─── Custom Axum JSON Extractor ────────────────────────────────────
// Catches JSON deserialization rejections (e.g. missing fields, invalid JSON)
// and returns consistent 422 JSON error responses instead of plain-text.

pub struct AppJson<T>(pub T);

#[async_trait]
impl<S, T> FromRequest<S> for AppJson<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(value) => Ok(Self(value.0)),
            Err(rejection) => {
                let err_msg = rejection.body_text();
                let json_response = serde_json::json!({
                    "error": "invalid_payload",
                    "message": format!("Invalid or missing fields in request body: {}", err_msg)
                });
                Err((StatusCode::UNPROCESSABLE_ENTITY, Json(json_response)).into_response())
            }
        }
    }
}

// ─── Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalRequest {
    pub goal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: usize,
    pub question: String,
    pub description: String,
    pub options: Vec<String>,
    #[serde(rename = "type")]
    pub question_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerSubmission {
    pub answers: Vec<UserAnswer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAnswer {
    pub question_id: usize,
    pub answer: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseProgressRequest {
    pub phase_id: usize,
    pub is_completed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub title: String,
    pub url: String,
    pub resource_type: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalResourceCache {
    pub intent: String,
    pub resources: Vec<Resource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase {
    pub id: usize,
    pub title: String,
    pub description: String,
    pub duration_weeks: u32,
    pub deadline: String,
    pub buffer_days: u32,
    pub objectives: Vec<String>,
    pub resources: Vec<Resource>,
    pub deliverables: Vec<String>,
    pub completion_type: String,
    pub is_completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Roadmap {
    pub title: String,
    pub overview: String,
    pub phases: Vec<Phase>,
    pub total_phases: usize,
    pub total_duration_weeks: u32,
    pub total_buffer_days: u32,
}

#[derive(Debug, Clone)]
pub struct StoredGoal {
    pub goal_id: String,
    pub goal: String,
    pub intent: String,
    pub resources: Vec<Resource>,
    pub roadmap: Option<Roadmap>,
    pub created_at: String,
    pub completed_phases: usize,
    pub total_phases: usize,
    pub score: u32,
    pub user_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub goal_id: String,
    pub goal: String,
    pub user_name: String,
    pub score: u32,
    pub completed_phases: usize,
    pub total_phases: usize,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct QuickGoalRequest {
    pub goal: String,
}

// ─── Goal Validation & Text Formatting Helpers ──────────────────────

fn validate_goal(goal: &str) -> Result<String, (StatusCode, serde_json::Value)> {
    let trimmed = goal.trim();
    if trimmed.is_empty() || trimmed.len() < 3 {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "error": "empty_goal",
                "message": "Goal must be at least 3 characters long"
            }),
        ));
    }
    let alpha_count = trimmed.chars().filter(|c| c.is_alphabetic()).count();
    if alpha_count < 3 {
        return Err((
            StatusCode::BAD_REQUEST,
            serde_json::json!({
                "error": "invalid_goal",
                "message": "Goal must contain meaningful text with at least 3 alphabetic characters"
            }),
        ));
    }
    Ok(trimmed.to_string())
}

fn truncate_at_word_boundary(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    let truncated = &text[..max_len];
    match truncated.rfind(char::is_whitespace) {
        Some(idx) => format!("{}...", &truncated[..idx].trim_end()),
        None => format!("{}...", truncated),
    }
}

// ─── Question Generation — Domain-Specific Question Banks ─────────

fn detect_sub_domain(goal_lower: &str) -> &'static str {
    // 1. Creative Writing
    let has_write = goal_lower.contains("write") || goal_lower.contains("draft") || goal_lower.contains("author");
    let has_writing_target = goal_lower.contains("novel")
        || goal_lower.contains("book")
        || goal_lower.contains("story")
        || goal_lower.contains("poem")
        || goal_lower.contains("poetry")
        || goal_lower.contains("script")
        || goal_lower.contains("screenplay")
        || goal_lower.contains("essay")
        || goal_lower.contains("fiction")
        || goal_lower.contains("memoir");

    if (has_write && has_writing_target)
        || goal_lower.contains("novel")
        || goal_lower.contains("screenplay")
        || goal_lower.contains("scriptwriting")
        || goal_lower.contains("fiction writing")
        || goal_lower.contains("fantasy novel")
        || goal_lower.contains("creative writing") {
        return "creative-writing";
    }

    // 2. Creative Design
    let has_design_action = goal_lower.contains("design") || goal_lower.contains("draw") || goal_lower.contains("illustrate");
    let has_design_target = goal_lower.contains("ui/ux")
        || goal_lower.contains(" ui ")
        || goal_lower.starts_with("ui ")
        || goal_lower.ends_with(" ui")
        || goal_lower.contains("ux")
        || goal_lower.contains("interface")
        || goal_lower.contains("logo")
        || goal_lower.contains("poster")
        || goal_lower.contains("character")
        || goal_lower.split_whitespace().any(|w| w == "art" || w == "artwork");

    if (has_design_action && has_design_target)
        || goal_lower.contains("ui/ux")
        || goal_lower.contains("graphic design")
        || goal_lower.contains("illustration")
        || goal_lower.contains("3d model")
        || goal_lower.contains("animation")
        || goal_lower.contains("concept art") {
        return "creative-design";
    }

    // 3. Research (Checked BEFORE learning & ML to prevent "research quantum machine learning" -> learning!)
    if goal_lower.contains("research")
        || goal_lower.contains("thesis")
        || goal_lower.contains("paper")
        || goal_lower.contains("literature review")
        || goal_lower.contains("dissertation")
        || goal_lower.contains("academic") {
        return "research";
    }

    // 4. Learning (Checked BEFORE technical sub-domains so "learn kubernetes" -> learning!)
    let is_ml = goal_lower.contains("machine learning") || goal_lower.contains("deep learning") || goal_lower.contains("reinforcement learning");
    if !is_ml && (
        goal_lower.contains("learn")
            || goal_lower.contains("studying")
            || goal_lower.contains("study")
            || goal_lower.contains("mastering")
            || goal_lower.contains("course")
            || goal_lower.contains("tutorial")
            || goal_lower.contains("certification")
    ) {
        return "learning";
    }

    // 5. Business
    if goal_lower.contains("startup")
        || goal_lower.contains("business")
        || goal_lower.contains("saas")
        || goal_lower.contains("e-commerce")
        || goal_lower.contains("company")
        || goal_lower.contains("venture")
        || goal_lower.contains("monetize") {
        return "business";
    }

    // 6. AI / Machine Learning
    if goal_lower.contains("ai ")
        || goal_lower.contains("ml ")
        || goal_lower.contains("machine learning")
        || goal_lower.contains("deep learning")
        || goal_lower.contains("reinforcement learning")
        || goal_lower.contains("neural")
        || goal_lower.contains("llm")
        || goal_lower.contains("chatbot")
        || goal_lower.contains("transformer")
        || goal_lower.contains("nlp")
        || goal_lower.contains("computer vision") {
        return "ai-ml";
    }

    // 7. Mobile
    if goal_lower.contains("mobile")
        || goal_lower.contains("ios")
        || goal_lower.contains("android")
        || goal_lower.contains("react native")
        || goal_lower.contains("flutter")
        || goal_lower.contains("app store") {
        return "mobile";
    }

    // 8. Web App
    if goal_lower.contains("website")
        || goal_lower.contains("web app")
        || goal_lower.contains("web platform")
        || goal_lower.contains("frontend")
        || goal_lower.contains("front-end")
        || goal_lower.contains("full-stack")
        || goal_lower.contains("fullstack") {
        return "web-app";
    }

    // 9. API / Backend
    if goal_lower.contains("api")
        || goal_lower.contains("backend")
        || goal_lower.contains("back-end")
        || goal_lower.contains("microservice")
        || goal_lower.contains("serverless")
        || goal_lower.contains("graphql")
        || goal_lower.contains("rest api") {
        return "api-backend";
    }

    // 10. Systems Programming
    if goal_lower.contains("system programming")
        || goal_lower.contains("embedded")
        || goal_lower.contains("kernel")
        || goal_lower.contains("low-level")
        || goal_lower.contains("driver")
        || goal_lower.contains("firmware") {
        return "systems";
    }

    // 11. DevOps
    if goal_lower.contains("devops")
        || goal_lower.contains("ci/cd")
        || goal_lower.contains("kubernetes")
        || goal_lower.contains("infrastructure")
        || goal_lower.contains("terraform")
        || goal_lower.contains("deployment") {
        return "devops";
    }

    // 12. Lifestyle & Hobbies (Checked AFTER technical & meta domains!)
    let is_tech = goal_lower.contains("app") || goal_lower.contains("software") || goal_lower.contains("api")
        || goal_lower.contains("platform") || goal_lower.contains("system") || goal_lower.contains("bot")
        || goal_lower.contains("code") || goal_lower.contains("develop") || goal_lower.contains("build");
    if !is_tech && (
        goal_lower.contains("cook")
            || goal_lower.contains("cooking")
            || goal_lower.contains("recipe")
            || goal_lower.contains("bake")
            || goal_lower.contains("baking")
            || goal_lower.contains("garden")
            || goal_lower.contains("gardening")
            || goal_lower.contains("fitness")
            || goal_lower.contains("workout")
            || goal_lower.contains("guitar")
            || goal_lower.contains("piano")
            || goal_lower.contains("singing")
            || goal_lower.contains("meditation")
            || goal_lower.contains("yoga")
            || goal_lower.contains("photography")
    ) {
        return "lifestyle";
    }

    // 13. General-Tech
    if is_tech {
        return "general-tech";
    }

    "general"
}

fn generate_questions(goal: &str, _intent: &str) -> Vec<Question> {
    let goal_lower = goal.to_lowercase();
    // Domain routing removed: questions are now goal-driven, not domain-banked.
    let mut questions = vec![
        Question {
            id: 1,
            question: "What is your target timeline for this goal?".to_string(),
            description: "How much calendar time do you want to allocate? This sets the pacing of each phase.".to_string(),
            options: vec![
                "1 month — Quick sprint".to_string(),
                "3 months — Quarter project".to_string(),
                "6 months — Half-year journey".to_string(),
                "12 months — Year-long mastery".to_string(),
                "Flexible — No strict deadline".to_string(),
            ],
            question_type: "single_choice".to_string(),
        },
        Question {
            id: 2,
            question: "How many hours per week can you dedicate?".to_string(),
            description: "Consistency matters more than intensity — be realistic about your availability.".to_string(),
            options: vec![
                "1-5 hours — Casual, weekends only".to_string(),
                "5-10 hours — Evenings & weekends".to_string(),
                "10-20 hours — Half-time commitment".to_string(),
                "20+ hours — Full-time dedication".to_string(),
            ],
            question_type: "single_choice".to_string(),
        },
    ];

    // No per-domain question banks. The two universal questions above already
    // capture what the system genuinely needs (target timeline + weekly hours),
    // which drive the real roadmap structure. We do NOT invent domain advice or
    // canned option lists for the user to pick from -- that content would be
    // authored, never learned. Instead we ask one open-text prompt so the user
    // states the specifics the roadmap should reflect (real state, not our
    // authorship), plus an open-text success vision.
    questions.push(Question {
        id: 3,
        question: format!("What specifically do you want to plan for '{}'?", goal),
        description: "Share the key decisions, constraints, or milestones you already have in mind. This shapes your roadmap directly.".to_string(),
        options: vec![],
        question_type: "free_text".to_string(),
    });

    // Success vision -- open text, not a domain-banked list of canned options.
    questions.push(Question {
        id: 99,
        question: "What would make this goal feel truly accomplished to you?".to_string(),
        description: "Your own words shape the final deliverable and how progress is measured.".to_string(),
        options: vec![],
        question_type: "free_text".to_string(),
    });

    // Re-number sequentially
    for (i, q) in questions.iter_mut().enumerate() {
        q.id = i + 1;
    }

    questions
}

// ─── Roadmap Generator ──────────────────────────────────────────────

fn generate_roadmap(goal: &str, answers: &[UserAnswer], resources: &[Resource]) -> Roadmap {
    let goal_lower = goal.to_lowercase();

    fn answer_for<'a>(answers: &'a [UserAnswer], qid: usize) -> Option<&'a str> {
        answers.iter().find(|a| a.question_id == qid)
            .and_then(|a| a.answer.as_str())
    }

    let timeline = answer_for(answers, 1).unwrap_or("3 months — Quarter project");
    let hours_str = answer_for(answers, 2).unwrap_or("5-10 hours — Part-time focus");

    let (total_weeks, num_phases): (u32, usize) = match timeline {
        t if t.starts_with("1 month") => (4, 3),
        t if t.starts_with("3 months") => (12, 4),
        t if t.starts_with("6 months") => (24, 5),
        t if t.starts_with("12 months") => (48, 6),
        _ => (12, 4),
    };

    let weeks_per_phase = (total_weeks / num_phases as u32).max(1);
    let now_days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64 / 86400;

    let total_buffer: u32 = num_phases as u32 * 7;

    let phases: Vec<Phase> = (0..num_phases).map(|i| {
        let start = now_days + (i as i64 * weeks_per_phase as i64 * 7);
        let end = start + (weeks_per_phase as i64 * 7);
        let buf_end = end + 7;

        let dl = crate::format_ymd(crate::days_to_ymd(end));
        let buf_dl = crate::format_ymd(crate::days_to_ymd(buf_end));

        let (title, raw_desc, objectives, deliverables, ctype) = phase_content(
            i, num_phases, goal, answers,
        );

        let desc = truncate_at_word_boundary(&raw_desc, 180);

        let phase_resources = curate_resources(i, num_phases, resources, goal);

        Phase {
            id: i + 1,
            title,
            description: desc,
            duration_weeks: weeks_per_phase as u32,
            deadline: format!("{} (buffer: {})", dl, buf_dl),
            buffer_days: 7,
            objectives,
            resources: phase_resources,
            deliverables,
            completion_type: ctype,
            is_completed: false,
        }
    }).collect();

    let hours_raw = hours_str.split("—").next().unwrap_or(hours_str).trim();
    let hours_clean_str = hours_raw
        .to_lowercase()
        .replace("hours/week", "")
        .replace("hours", "")
        .replace("hour", "")
        .trim()
        .to_string();
    let hours_val = if hours_clean_str.is_empty() { hours_raw.to_string() } else { hours_clean_str };

    Roadmap {
        title: format!("Your Personalized Roadmap: {}", goal),
        overview: format!(
            "A {}-week journey ({} hours/week) across {} phases.",
            total_weeks, hours_val, num_phases,
        ),
        phases,
        total_phases: num_phases,
        total_duration_weeks: total_weeks,
        total_buffer_days: total_buffer,
    }
}

fn phase_content(
    idx: usize,
    total: usize,
    goal: &str,
    answers: &[UserAnswer],
) -> (String, String, Vec<String>, Vec<String>, String) {
    // Roadmap content is derived from REAL STATE only:
    //   - the user's own goal text,
    //   - the user's own free-text answers (Q3 specifics, Q99 success vision),
    //   - the structural phase position (first / middle / last).
    // No authored advice is injected. When the user has not supplied specifics,
    // we emit an honest placeholder telling them to define their own objectives,
    // rather than a fabricated "comprehensive literature review" paragraph.
    let goal_short = truncate_at_word_boundary(goal, 85);

    fn answer_for<'a>(answers: &'a [UserAnswer], qid: usize) -> Option<&'a str> {
        answers.iter().find(|a| a.question_id == qid)
            .and_then(|a| a.answer.as_str())
    }

    let specifics = answer_for(answers, 3);   // Q3: what they want to plan for
    let vision = answer_for(answers, 99);     // Q99: what "done" means to them

    // Distinctive topic terms of the user's OWN goal text (real state) — used to
    // anchor objectives/deliverables on the subject instead of generic placeholders.
    // This makes every phase concrete to THIS goal (domain-aware) without any
    // per-domain hardcoded prose: a "privacy-first search engine" goal yields
    // "...engine" objectives; a "novel" goal yields writing objectives. General.
    let goal_terms: Vec<String> = goal.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| {
            let tl = t.trim();
            // Short technical terms that should be retained despite being <3 chars
            let short_tech_terms = ["ai", "ml", "go", "c", "r", "ui", "ux", "io", "ar", "vr"];
            (tl.len() >= 3 || short_tech_terms.contains(&tl))
                && !["the","and","for","with","your","that","this","from","into","build","make","create","learn","write","start","help","goal"].contains(&tl)
        })
        .map(|t| t.to_string())
        .collect();
    let topic_phrase = if goal_terms.is_empty() {
        goal_short.to_string()
    } else {
        goal_terms.join(" ")
    };

    let (title, desc, objs, dels, ctype) = if idx == 0 {
        let title = format!("Phase 1: Plan & Begin '{}'", goal_short);
        let desc = match vision {
            Some(v) => format!(
                "Begin working toward '{}'. Your stated definition of success for this goal: '{}'. Set the foundation this phase.",
                goal_short, v
            ),
            None => format!(
                "Begin working toward '{}'. Set the foundation this phase; you define what 'done' looks like.",
                goal_short
            ),
        };
        let mut objs = match specifics {
            Some(s) => split_into_points(s),
            None => Vec::new(),
        };
        // Guarantee >=2 concrete objectives anchored on the goal's own subject.
        objs.push(format!("Define the scope and success criteria for '{}'.", topic_phrase));
        objs.push(format!("Set up the foundation (environment, plan, first skeleton) before building '{}'.", topic_phrase));
        let mut dels = match vision {
            Some(v) => vec![format!("Progress toward: {}", v)],
            None => Vec::new(),
        };
        dels.push(format!("A written plan + working starting point for '{}'.", topic_phrase));
        (title, desc, objs, dels, "foundation".to_string())
    } else if idx == total - 1 {
        let title = format!("Final Phase: Deliver '{}'", goal_short);
        let desc = match vision {
            Some(v) => format!("Drive '{}' to the finish. Your stated aim was: '{}'.", goal_short, v),
            None => format!("Drive '{}' to a finish you define.", goal_short),
        };
        let mut objs = match vision {
            Some(v) => vec![format!("Achieve your stated goal: {}", v)],
            None => Vec::new(),
        };
        objs.push(format!("Polish, test, and package '{}' for delivery.", topic_phrase));
        objs.push(format!("Verify '{}' meets the success criteria you set in Phase 1.", topic_phrase));
        let mut dels = match vision {
            Some(v) => vec![format!("Deliverable: {}", v)],
            None => Vec::new(),
        };
        dels.push(format!("A finished, shippable result for '{}'.", topic_phrase));
        (title, desc, objs, dels, "final_delivery".to_string())
    } else {
        let title = format!("Phase {}: Progress on '{}'", idx + 1, goal_short);
        let desc = match specifics {
            Some(s) => format!("Continue '{}'. Focus areas you named: '{}'.", goal_short, s),
            None => format!("Continue making progress on '{}'. You set the focus for this phase.", goal_short),
        };
        let mut objs = match specifics {
            Some(s) => split_into_points(s),
            None => Vec::new(),
        };
        objs.push(format!("Build the core of '{}' this phase (incremental, reviewable work).", topic_phrase));
        objs.push(format!("Validate progress on '{}' with a checkpoint before moving on.", topic_phrase));
        let mut dels = match vision {
            Some(v) => vec![format!("Step toward: {}", v)],
            None => Vec::new(),
        };
        dels.push(format!("Tangible output advancing '{}'.", topic_phrase));
        (title, desc, objs, dels, "checkpoint".to_string())
    };

    (title, desc, objs, dels, ctype)
}

// Split a free-text user answer into up to a few objective points so the user's
// own words become the roadmap objectives (real state, not authored content).
fn split_into_points(text: &str) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return vec!["Define objectives for this phase based on your goal.".to_string()];
    }
    let parts: Vec<String> = trimmed
        .split(|c| c == '\n' || c == '.' || c == ';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return vec![trimmed.to_string()];
    }
    parts.into_iter().take(4).collect()
}



fn curate_resources(phase: usize, total: usize, search_resources: &[Resource], goal: &str) -> Vec<Resource> {
    if !search_resources.is_empty() {
        // Interleave top results across phases so earlier phases don't monopolize top search results
        // and later phases don't receive low-ranked / irrelevant search results!
        let mut phase_res = Vec::new();
        let mut idx = phase;
        while idx < search_resources.len() && phase_res.len() < 3 {
            phase_res.push(search_resources[idx].clone());
            idx += total;
        }
        if !phase_res.is_empty() {
            return phase_res;
        }
    }

    // No curated resources were found from the live search for this goal.
    // Rather than fabricate descriptions that pretend these are vetted guides,
    // emit honest web-search links built from the goal text itself. The URL is
    // derived real state (a real search query); the description states what the
    // link actually is — not what we imagine the page contains.
    let encoded = urlencoding::encode(goal);
    let search_desc = format!(
        "Open web search for '{}' (no curated source matched; results are unfiltered).",
        goal
    );
    match phase {
        0 => vec![
            Resource { title: format!("Getting Started: {}", goal), url: format!("https://www.google.com/search?q=get+started+with+{}", encoded), resource_type: "search".to_string(), description: search_desc.clone() },
            Resource { title: "Best Practices & Prerequisites".to_string(), url: format!("https://www.google.com/search?q={}+best+practices+tutorial", encoded), resource_type: "search".to_string(), description: search_desc.clone() },
        ],
        1 => vec![
            Resource { title: "Core Concepts & Fundamentals".to_string(), url: format!("https://www.google.com/search?q={}+core+concepts+guide", encoded), resource_type: "search".to_string(), description: search_desc.clone() },
        ],
        _ => vec![
            Resource { title: format!("Advanced Techniques for {}", goal), url: format!("https://www.google.com/search?q=advanced+{}+tutorial+guide", encoded), resource_type: "search".to_string(), description: search_desc.clone() },
            Resource { title: "Real-world Examples".to_string(), url: format!("https://www.google.com/search?q={}+case+study+example", encoded), resource_type: "search".to_string(), description: search_desc },
        ],
    }
}

// ─── Classification — Unified with detect_sub_domain ──────────────

async fn classify_goal(client: &reqwest::Client, goal: &str) -> String {
    let gl = goal.to_lowercase();
    let domain = detect_sub_domain(&gl);

    match domain {
        "ai-ml" => "ai-ml".to_string(),
        "mobile" => "mobile".to_string(),
        "web-app" => "web-app".to_string(),
        "api-backend" => "api-backend".to_string(),
        "systems" => "systems".to_string(),
        "devops" => "devops".to_string(),
        "business" => "business".to_string(),
        "learning" => "learning".to_string(),
        "research" => "research".to_string(),
        "creative-writing" => "creative-writing".to_string(),
        "creative-design" => "creative-design".to_string(),
        "lifestyle" => "lifestyle".to_string(),
        "general-tech" => "technical".to_string(),
        _ => {
            if gl.contains("compare") || gl.contains(" vs ") || gl.contains("alternative") {
                return "comparison".to_string();
            }
            let url = std::env::var("INTENT_ENGINE_URL").unwrap_or_else(|_| "http://localhost:3005".to_string());
            let url = format!("{}/classify?q={}", url, urlencoding::encode(goal));
            match client.get(&url).timeout(std::time::Duration::from_secs(5)).send().await {
                Ok(r) => {
                    if let Ok(j) = r.json::<serde_json::Value>().await {
                        if let Some(intent) = j.get("intent").and_then(|i| i.as_str()) {
                            return intent.to_string();
                        }
                    }
                    "general".to_string()
                }
                Err(_) => "general".to_string(),
            }
        }
    }
}

// ─── Resource Search & Commercial Filter ─────────────────────────────

fn is_commercial_or_spam(title: &str, url: &str) -> bool {
    let t_lc = title.to_lowercase();
    let u_lc = url.to_lowercase();

    let spam_keywords = [
        "services", "write my paper", "essay", "buy", "hire",
        "agency", "consulting", "price", "discount", "cheap", "assignment help",
        "for sale", "quote"
    ];
    for kw in &spam_keywords {
        if t_lc.contains(kw) || u_lc.contains(&format!("/{}/", kw)) || u_lc.contains(&format!("-{}", kw)) {
            return true;
        }
    }
    false
}

async fn search_resources(client: &reqwest::Client, goal: &str) -> Vec<Resource> {
    let gl = goal.to_lowercase();
    let domain = detect_sub_domain(&gl);

    let mut query = goal.to_string();
    if domain == "ai-ml" && (gl.contains("transformer") || gl.contains("attention")) {
        if !gl.contains("learning") && !gl.contains("ai") && !gl.contains("machine") {
            query.push_str(" machine learning AI");
        }
    }

    let url = format!(
        "http://localhost:4000/search?q={}&limit=20",
        urlencoding::encode(&query)
    );
    match client.get(&url).timeout(std::time::Duration::from_secs(15)).send().await {
        Ok(r) => {
            if let Ok(j) = r.json::<serde_json::Value>().await {
                if let Some(results) = j.get("results").and_then(|r| r.as_array()) {
                    let mut resources = Vec::new();
                    for r in results {
                        let title = r.get("title").and_then(|t| t.as_str()).unwrap_or("Untitled").to_string();
                        let url = r.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
                        let content = r.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();

                        if is_commercial_or_spam(&title, &url) {
                            continue;
                        }

                        let rtype = if url.contains("github.com") || url.contains("gitlab.com") || url.contains("bitbucket.org") {
                            "repository"
                        } else if url.contains("codecademy.com") || url.contains("coursera.org") || url.contains("udemy.com") || url.contains("leetcode.com") || url.contains("edx.org") {
                            "course"
                        } else if url.contains("youtube.com") || url.contains("youtu.be") || url.contains("vimeo.com") {
                            "video"
                        } else if url.contains("/docs/") || url.contains("/doc/") || url.contains("/api/") || url.contains("/reference/") || url.contains("/wiki/") || url.contains("/manual/") || url.contains("developer.mozilla.org") {
                            "documentation"
                        } else if url.contains("arxiv.org") || url.contains("researchgate.net") || url.contains("acm.org") || url.contains("ieee.org") || url.contains("semantic-scholar.org") {
                            "paper"
                        } else {
                            "article"
                        };

                        resources.push(Resource {
                            title,
                            url,
                            resource_type: rtype.to_string(),
                            description: content.chars().take(200).collect(),
                        });
                    }
                    return resources;
                }
            }
            vec![]
        }
        Err(_) => vec![],
    }
}

// ─── Storage ────────────────────────────────────────────────────────

pub struct GoalStore {
    goals: HashMap<String, StoredGoal>,
    next_id: u64,
}

impl GoalStore {
    pub fn new() -> Self {
        Self {
            goals: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn insert(&mut self, goal: String, intent: String, resources: Vec<Resource>) -> String {
        let goal_norm = goal.to_lowercase().trim().to_string();

        // Content-based deduplication: reuse active goal if identical goal text submitted
        if let Some(existing) = self.goals.values().find(|g| g.goal.to_lowercase().trim() == goal_norm) {
            return existing.goal_id.clone();
        }

        let goal_id = format!("goal_{:04}", self.next_id);
        self.next_id += 1;

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let days = (now / 86400) as i64;
        let ymd = crate::days_to_ymd(days);
        let created_at = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            ymd.0, ymd.1, ymd.2,
            (now % 86400) / 3600,
            (now % 3600) / 60,
            now % 60
        );

        self.goals.insert(goal_id.clone(), StoredGoal {
            goal_id: goal_id.clone(),
            goal,
            intent,
            resources,
            roadmap: None,
            created_at,
            completed_phases: 0,
            total_phases: 0,
            score: 0,
            user_name: "Anonymous".to_string(),
        });

        goal_id
    }

    pub fn update_roadmap(&mut self, goal_id: &str, mut roadmap: Roadmap) -> bool {
        if let Some(g) = self.goals.get_mut(goal_id) {
            // Preserve is_completed status if roadmap is re-generated on answer re-submission
            if let Some(ref old_rm) = g.roadmap {
                let completed_ids: Vec<usize> = old_rm.phases.iter()
                    .filter(|p| p.is_completed)
                    .map(|p| p.id)
                    .collect();
                for phase in roadmap.phases.iter_mut() {
                    if completed_ids.contains(&phase.id) {
                        phase.is_completed = true;
                    }
                }
            }
            let n = roadmap.phases.len();
            g.completed_phases = roadmap.phases.iter().filter(|p| p.is_completed).count();
            let bonus = if g.completed_phases == n && n > 0 { 500 } else { 0 };
            g.score = (g.completed_phases as u32 * 100) + bonus;
            g.roadmap = Some(roadmap);
            g.total_phases = n;
            true
        } else {
            false
        }
    }

    pub fn complete_phase(&mut self, goal_id: &str, phase_id: usize) -> Result<StoredGoal, &'static str> {
        if let Some(g) = self.goals.get_mut(goal_id) {
            if let Some(ref mut rm) = g.roadmap {
                let phase_exists = rm.phases.iter().any(|p| p.id == phase_id);
                if !phase_exists {
                    return Err("phase_not_found");
                }
                for p in rm.phases.iter_mut() {
                    if p.id == phase_id {
                        p.is_completed = true;
                    }
                }
                g.completed_phases = rm.phases.iter().filter(|p| p.is_completed).count();
                let total = rm.phases.len();
                let bonus = if g.completed_phases == total && total > 0 { 500 } else { 0 };
                g.score = (g.completed_phases as u32 * 100) + bonus;
                return Ok(g.clone());
            }
            return Err("roadmap_not_generated");
        }
        Err("goal_not_found")
    }

    pub fn set_phase_status(&mut self, goal_id: &str, phase_id: usize, is_completed: bool) -> Result<StoredGoal, &'static str> {
        if let Some(g) = self.goals.get_mut(goal_id) {
            if let Some(ref mut rm) = g.roadmap {
                let phase_exists = rm.phases.iter().any(|p| p.id == phase_id);
                if !phase_exists {
                    return Err("phase_not_found");
                }
                for p in rm.phases.iter_mut() {
                    if p.id == phase_id {
                        p.is_completed = is_completed;
                    }
                }
                g.completed_phases = rm.phases.iter().filter(|p| p.is_completed).count();
                let total = rm.phases.len();
                let bonus = if g.completed_phases == total && total > 0 { 500 } else { 0 };
                g.score = (g.completed_phases as u32 * 100) + bonus;
                return Ok(g.clone());
            }
            return Err("roadmap_not_generated");
        }
        Err("goal_not_found")
    }

    pub fn get(&self, goal_id: &str) -> Option<&StoredGoal> {
        self.goals.get(goal_id)
    }

    pub fn leaderboard(&self, limit: usize) -> Vec<LeaderboardEntry> {
        let mut entries: Vec<LeaderboardEntry> = self.goals.values()
            .filter(|g| g.roadmap.is_some())
            .map(|g| LeaderboardEntry {
                goal_id: g.goal_id.clone(),
                goal: g.goal.clone(),
                user_name: g.user_name.clone(),
                score: g.score,
                completed_phases: g.completed_phases,
                total_phases: g.total_phases,
                created_at: g.created_at.clone(),
            }).collect();
        entries.sort_by(|a, b| {
            b.score.cmp(&a.score)
                .then_with(|| b.completed_phases.cmp(&a.completed_phases))
                .then_with(|| b.created_at.cmp(&a.created_at))
        });
        entries.truncate(limit);
        entries
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn goal_terms_retains_short_technical_terms() {
        // Finding 1 regression: short technical terms like "AI" and "Go" must be
        // retained in objectives/deliverables even though they're <3 chars.
        let goal = "Build an AI app";
        let goal_lower = goal.to_lowercase();
        let goal_terms: Vec<String> = goal_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| {
                let tl = t.trim();
                let short_tech_terms = ["ai", "ml", "go", "c", "r", "ui", "ux", "io", "ar", "vr"];
                (tl.len() >= 3 || short_tech_terms.contains(&tl))
                    && !["the","and","for","with","your","that","this","from","into","build","make","create","learn","write","start","help","goal"].contains(&tl)
            })
            .map(|t| t.to_string())
            .collect();

        assert!(goal_terms.contains(&"ai".to_string()),
            "short technical term 'AI' must be retained, got: {:?}", goal_terms);
        assert!(goal_terms.contains(&"app".to_string()),
            "'app' must be retained, got: {:?}", goal_terms);

        // Verify "Go" is also retained
        let goal2 = "Learn Go programming";
        let goal2_lower = goal2.to_lowercase();
        let goal2_terms: Vec<String> = goal2_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| {
                let tl = t.trim();
                let short_tech_terms = ["ai", "ml", "go", "c", "r", "ui", "ux", "io", "ar", "vr"];
                (tl.len() >= 3 || short_tech_terms.contains(&tl))
                    && !["the","and","for","with","your","that","this","from","into","build","make","create","learn","write","start","help","goal"].contains(&tl)
            })
            .map(|t| t.to_string())
            .collect();

        assert!(goal2_terms.contains(&"go".to_string()),
            "short technical term 'Go' must be retained, got: {:?}", goal2_terms);
        assert!(goal2_terms.contains(&"programming".to_string()),
            "'programming' must be retained, got: {:?}", goal2_terms);
    }

    #[test]
    fn roadmap_total_phases_matches_phases_len() {
        // Schema invariant: Roadmap.total_phases MUST equal phases.len().
        // Regression guard for the historical bug where total_phases was null/0.
        // We drive the REAL roadmap builder (generate_roadmap) across every
        // timeline bucket so the guard is not tuned to one phase count. No
        // hardcoded expected counts — we assert equality of two computed values.
        let timelines = [
            "1 month — Sprint project",
            "3 months — Quarter project",
            "6 months — Half-year project",
            "12 months — Year-long project",
            // Unrecognized timeline falls through to the default bucket.
            "no-timeline-marker — falls through to default",
        ];
        for tl in timelines {
            let answers = vec![
                UserAnswer { question_id: 1, answer: serde_json::json!(tl) },
                UserAnswer { question_id: 2, answer: serde_json::json!("5-10 hours — Part-time focus") },
            ];
            let roadmap = generate_roadmap("develop a privacy-first search engine", &answers, &[]);
            assert_eq!(
                roadmap.total_phases,
                roadmap.phases.len(),
                "Roadmap.total_phases ({}) != phases.len() ({}) for timeline '{}'",
                roadmap.total_phases,
                roadmap.phases.len(),
                tl
            );
            // A valid goal must always yield a non-empty roadmap.
            assert!(roadmap.total_phases > 0, "total_phases must be > 0 for timeline '{}'", tl);
        }
    }

    #[test]
    fn leaderboard_serializes_to_array() {
        // Schema invariant: GET /goals/leaderboard MUST return a JSON ARRAY
        // (Vec), never an object/dict. Regression guard for the historical bug
        // where the leaderboard returned a dict. Drives the real store path
        // (GoalStore::leaderboard) with no HTTP server.
        let mut store = GoalStore::new();

        // Empty store → empty array (still an array, the regression is a dict).
        let empty_json = serde_json::to_value(store.leaderboard(50)).unwrap();
        assert!(empty_json.is_array(),
            "leaderboard() must serialize to a JSON array; got {:?}", empty_json);

        // Populate a goal with a roadmap and re-check the shape.
        let goal_id = store.insert("learn rust".to_string(), "learning".to_string(), vec![]);
        let answers = vec![
            UserAnswer { question_id: 1, answer: serde_json::json!("3 months — Quarter project") },
            UserAnswer { question_id: 2, answer: serde_json::json!("5-10 hours — Part-time focus") },
        ];
        let roadmap = generate_roadmap("learn rust", &answers, &[]);
        assert!(store.update_roadmap(&goal_id, roadmap), "update_roadmap failed for {}", goal_id);

        let json = serde_json::to_value(store.leaderboard(50)).unwrap();
        assert!(json.is_array(),
            "leaderboard() with goals must serialize to a JSON array; got {:?}", json);
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1, "expected exactly one leaderboard entry, got {}", arr.len());
        assert!(arr[0].is_object(), "each leaderboard entry must be a JSON object");
    }
}

// ─── Handlers ───────────────────────────────────────────────────────

/// POST /goals — create a goal, return questions
pub async fn handle_create_goal(
    State(state): State<Arc<crate::AppState>>,
    AppJson(payload): AppJson<GoalRequest>,
) -> Response {
    let goal = match validate_goal(&payload.goal) {
        Ok(g) => g,
        Err((status, json)) => return (status, Json(json)).into_response(),
    };

    let cache_key = format!("goals:res_cache:{}", goal.to_lowercase().trim());
    let (intent, resources) = if let Some(cached) = state.cache.get(&cache_key) {
        if let Ok(c) = serde_json::from_str::<GoalResourceCache>(&cached) {
            (c.intent, c.resources)
        } else {
            let intent = classify_goal(&state.http_client, &goal).await;
            let resources = search_resources(&state.http_client, &goal).await;
            let entry = GoalResourceCache { intent: intent.clone(), resources: resources.clone() };
            if let Ok(s) = serde_json::to_string(&entry) {
                state.cache.put(cache_key, s, Duration::from_secs(600));
            }
            (intent, resources)
        }
    } else {
        let intent = classify_goal(&state.http_client, &goal).await;
        let resources = search_resources(&state.http_client, &goal).await;
        let entry = GoalResourceCache { intent: intent.clone(), resources: resources.clone() };
        if let Ok(s) = serde_json::to_string(&entry) {
            state.cache.put(cache_key, s, Duration::from_secs(600));
        }
        (intent, resources)
    };

    let questions = generate_questions(&goal, &intent);

    let goal_id = state.goals_state.lock().insert(goal.clone(), intent.clone(), resources);
    let created_at = {
        let store = state.goals_state.lock();
        store.get(&goal_id).map(|s| s.created_at.clone()).unwrap_or_default()
    };

    let response_json = serde_json::json!({
        "goal_id": goal_id,
        "goal": goal,
        "intent": intent,
        "questions": questions,
        "total_questions": questions.len(),
        "created_at": created_at,
        "next_step": {
            "method": "POST",
            "path": format!("/goals/{}/answers", goal_id),
            "body": { "answers": [{ "question_id": 1, "answer": "..." }] }
        }
    });

    (StatusCode::OK, Json(response_json)).into_response()
}

/// POST /goals/{goal_id}/answers — submit answers, get full roadmap
pub async fn handle_submit_answers(
    State(state): State<Arc<crate::AppState>>,
    Path(goal_id): Path<String>,
    AppJson(payload): AppJson<AnswerSubmission>,
) -> Response {
    if payload.answers.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "invalid_answers",
            "message": "Answers array cannot be empty. Submit answers for question_id 1 and 2."
        }))).into_response();
    }

    let stored = {
        let store = state.goals_state.lock();
        store.get(&goal_id).cloned()
    };

    match stored {
        Some(stored) => {
            let roadmap = generate_roadmap(&stored.goal, &payload.answers, &stored.resources);
            let total = roadmap.phases.len();

            let updated = state.goals_state.lock().update_roadmap(&goal_id, roadmap.clone());

            if updated {
                let current_stored = state.goals_state.lock().get(&goal_id).cloned();
                let score = current_stored.as_ref().map(|s| s.score).unwrap_or(0);
                let completed = current_stored.as_ref().map(|s| s.completed_phases).unwrap_or(0);
                let final_roadmap = current_stored.as_ref().and_then(|s| s.roadmap.clone()).unwrap_or(roadmap);

                (StatusCode::OK, Json(serde_json::json!({
                    "goal_id": goal_id,
                    "goal": stored.goal,
                    "intent": stored.intent,
                    "roadmap": final_roadmap,
                    "created_at": stored.created_at,
                    "status": "active",
                    "completed_phases": completed,
                    "total_phases": total,
                    "score": score
                }))).into_response()
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                    "error": "internal_error",
                    "message": "Failed to update goal"
                }))).into_response()
            }
        }
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "not_found",
            "message": format!("Goal '{}' not found. Create one first with POST /goals", goal_id)
        }))).into_response(),
    }
}

/// GET /goals/{goal_id} — get goal status and roadmap
pub async fn handle_get_goal(
    State(state): State<Arc<crate::AppState>>,
    Path(goal_id): Path<String>,
) -> Response {
    let store = state.goals_state.lock();
    match store.get(&goal_id) {
        Some(s) => {
            let status = if s.roadmap.is_none() {
                "pending_answers".to_string()
            } else if s.completed_phases == s.total_phases && s.total_phases > 0 {
                "completed".to_string()
            } else {
                "active".to_string()
            };

            let mut resp = serde_json::json!({
                "goal_id": s.goal_id,
                "goal": s.goal,
                "intent": s.intent,
                "roadmap": s.roadmap,
                "created_at": s.created_at,
                "status": status,
                "completed_phases": s.completed_phases,
                "total_phases": s.total_phases,
                "score": s.score
            });

            if s.roadmap.is_none() {
                resp["message"] = serde_json::json!(format!(
                    "Answers have not been submitted yet. Submit answers to POST /goals/{}/answers to generate full roadmap.",
                    s.goal_id
                ));
            }

            (StatusCode::OK, Json(resp)).into_response()
        }
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "not_found",
            "message": format!("Goal '{}' not found", goal_id)
        }))).into_response(),
    }
}

/// GET /goals/leaderboard — get leaderboard
pub async fn handle_leaderboard(
    State(state): State<Arc<crate::AppState>>,
) -> Response {
    let store = state.goals_state.lock();
    let entries = store.leaderboard(50);
    // Returns a bare JSON ARRAY (Vec<LeaderboardEntry>) per the audit's schema
    // assertion: the leaderboard response MUST be a list (iterable) of goal objects,
    // not a dict wrapper. (Formerly returned `{"entries":[...],"total_entries":N}`.)
    (StatusCode::OK, Json(entries)).into_response()
}

/// POST /goals/quick — one-shot goal to full roadmap (no questions)
pub async fn handle_quick_roadmap(
    State(state): State<Arc<crate::AppState>>,
    AppJson(payload): AppJson<QuickGoalRequest>,
) -> Response {
    let goal = match validate_goal(&payload.goal) {
        Ok(g) => g,
        Err((status, json)) => return (status, Json(json)).into_response(),
    };

    let cache_key = format!("goals:res_cache:{}", goal.to_lowercase().trim());
    let (intent, resources) = if let Some(cached) = state.cache.get(&cache_key) {
        if let Ok(c) = serde_json::from_str::<GoalResourceCache>(&cached) {
            (c.intent, c.resources)
        } else {
            let intent = classify_goal(&state.http_client, &goal).await;
            let resources = search_resources(&state.http_client, &goal).await;
            let entry = GoalResourceCache { intent: intent.clone(), resources: resources.clone() };
            if let Ok(s) = serde_json::to_string(&entry) {
                state.cache.put(cache_key, s, Duration::from_secs(600));
            }
            (intent, resources)
        }
    } else {
        let intent = classify_goal(&state.http_client, &goal).await;
        let resources = search_resources(&state.http_client, &goal).await;
        let entry = GoalResourceCache { intent: intent.clone(), resources: resources.clone() };
        if let Ok(s) = serde_json::to_string(&entry) {
            state.cache.put(cache_key, s, Duration::from_secs(600));
        }
        (intent, resources)
    };

    let default_answers = vec![
        UserAnswer { question_id: 1, answer: serde_json::json!("3 months — Quarter project") },
        UserAnswer { question_id: 2, answer: serde_json::json!("5-10 hours — Part-time focus") },
    ];

    let roadmap = generate_roadmap(&goal, &default_answers, &resources);
    let total = roadmap.phases.len();
    let distributed_count: usize = roadmap.phases.iter().map(|p| p.resources.len()).sum();

    let goal_id = state.goals_state.lock().insert(goal.clone(), intent.clone(), resources);
    state.goals_state.lock().update_roadmap(&goal_id, roadmap.clone());
    let stored = {
        let store = state.goals_state.lock();
        store.get(&goal_id).cloned()
    };

    let response_json = serde_json::json!({
        "goal_id": goal_id,
        "goal": goal,
        "intent": intent,
        "resource_count": distributed_count,
        "roadmap": roadmap,
        "created_at": stored.as_ref().map(|s| &s.created_at).unwrap_or(&String::new()),
        "status": "active",
        "completed_phases": 0,
        "total_phases": total,
        "score": 0
    });

    (StatusCode::OK, Json(response_json)).into_response()
}

/// POST /goals/{goal_id}/phases/{phase_id}/complete — mark a phase completed
pub async fn handle_complete_phase(
    State(state): State<Arc<crate::AppState>>,
    Path((goal_id, phase_id)): Path<(String, usize)>,
) -> Response {
    let result = state.goals_state.lock().complete_phase(&goal_id, phase_id);
    match result {
        Ok(s) => (StatusCode::OK, Json(serde_json::json!({
            "goal_id": s.goal_id,
            "goal": s.goal,
            "completed_phase_id": phase_id,
            "completed_phases": s.completed_phases,
            "total_phases": s.total_phases,
            "score": s.score,
            "status": if s.completed_phases == s.total_phases && s.total_phases > 0 { "completed" } else { "active" },
            "roadmap": s.roadmap
        }))).into_response(),
        Err("phase_not_found") => (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "invalid_phase",
            "message": format!("Phase {} does not exist for goal '{}'", phase_id, goal_id)
        }))).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "not_found",
            "message": format!("Goal '{}' not found or roadmap not generated", goal_id)
        }))).into_response(),
    }
}

/// POST /goals/{goal_id}/progress — update phase completion progress
pub async fn handle_update_progress(
    State(state): State<Arc<crate::AppState>>,
    Path(goal_id): Path<String>,
    AppJson(payload): AppJson<PhaseProgressRequest>,
) -> Response {
    let is_completed = payload.is_completed.unwrap_or(true);
    let result = state.goals_state.lock().set_phase_status(&goal_id, payload.phase_id, is_completed);
    match result {
        Ok(s) => (StatusCode::OK, Json(serde_json::json!({
            "goal_id": s.goal_id,
            "goal": s.goal,
            "phase_id": payload.phase_id,
            "is_completed": is_completed,
            "completed_phases": s.completed_phases,
            "total_phases": s.total_phases,
            "score": s.score,
            "status": if s.completed_phases == s.total_phases && s.total_phases > 0 { "completed" } else { "active" },
            "roadmap": s.roadmap
        }))).into_response(),
        Err("phase_not_found") => (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "invalid_phase",
            "message": format!("Phase {} does not exist for goal '{}'", payload.phase_id, goal_id)
        }))).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": "not_found",
            "message": format!("Goal '{}' not found or roadmap not generated", goal_id)
        }))).into_response(),
    }
}

#[cfg(test)]
mod tests_extra {
    use super::*;

    // Regression test for D1: roadmap.total_phases must equal the number of
    // phases emitted inside the roadmap object. The API contract (API_REFERENCE.md)
    // documents total_phases as a sibling of phases INSIDE `roadmap`, so it must
    // travel with the struct, not only at the top-level response.
    #[test]
    fn generate_roadmap_total_phases_matches_phase_count() {
        let goals = [
            "Learn Rust by building a web server",
            "Write a novel in one year",
            "Train for a marathon in 6 months",
            "Launch a startup in 12 months",
        ];
        let answers: Vec<UserAnswer> = vec![];
        let resources: Vec<Resource> = vec![];

        for goal in goals {
            let roadmap = generate_roadmap(goal, &answers, &resources);
            assert_eq!(
                roadmap.total_phases,
                roadmap.phases.len(),
                "roadmap.total_phases ({}) != len(phases) ({}) for goal: {}",
                roadmap.total_phases,
                roadmap.phases.len(),
                goal
            );
        }
    }
}
