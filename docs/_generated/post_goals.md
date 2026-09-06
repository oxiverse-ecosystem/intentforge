POST /goals

Creates a new goal and returns domain-specific questions tailored to the goal type.

**Request Body**

```json
{
  "goal": "build a full-stack web app for project management with team collaboration"
}
```

**Response: 200 OK**

```json
{
  "goal": "build a full-stack web app for project management with team collaboration",
  "goal_id": "g-abc123",
  "roadmap": {
    "phases": [
      {"id": 1, "name": "Research", "questions": ["What stack do you prefer?", "What is the deployment target?"]},
      {"id": 2, "name": "Design", "questions": ["What are the key features?", "Any design constraints?"]}
    ]
  },
  "next_step": 1
}
```

**Response: 400**

```json
{"error": "empty_goal", "message": "Goal must be at least 3 characters"}
```

Error codes: `empty_goal` (goal < 3 chars), `invalid_phase` (phase ID out of range), `not_found` (goal ID does not exist), `invalid_payload` (missing/invalid JSON body).
