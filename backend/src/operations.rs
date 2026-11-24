// Both the application and the dashboard use the router's templates, never raw URLs.
pub fn name(method: &str, route: &str) -> Option<&'static str> {
    Some(match (method, route) {
        ("POST", "/api/auth/login") => "Sign in",
        ("POST", "/api/auth/logout") => "Sign out",
        ("GET", "/api/auth/me") => "Check session",
        ("POST", "/api/auth/register") => "Register",
        ("POST", "/api/auth/register/resend") => "Resend verification",
        ("POST", "/api/auth/verify") => "Verify email",
        ("POST", "/api/practice/time") => "Save study time",
        ("GET", "/api/papers") => "Load library",
        ("POST", "/api/papers") => "Upload paper",
        ("POST", "/api/papers/generate") => "Generate paper",
        ("GET", "/api/papers/{paper_id}") => "Load paper",
        ("PATCH", "/api/papers/{paper_id}") => "Move paper",
        ("DELETE", "/api/papers/{paper_id}") => "Delete paper",
        ("POST", "/api/folders") => "Create folder",
        ("PATCH", "/api/folders/{folder_id}") => "Rename folder",
        ("DELETE", "/api/folders/{folder_id}") => "Delete folder",
        ("PUT", "/api/papers/{paper_id}/position") => "Switch question",
        ("POST", "/api/papers/{paper_id}/finish") => "Finish practice",
        ("PUT", "/api/papers/{paper_id}/answers/{question_id}") => "Submit answer",
        ("POST", "/api/papers/{paper_id}/answers/{question_id}/explain") => "Explain answer",
        ("POST", "/api/activity") => "Page action",
        ("GET", "/api/devops/access") => "Check DevOps access",
        ("GET", "/api/devops/logs") => "Read DevOps logs",
        ("GET", "/api/devops/cluster/{resource}") => "Read cluster status",
        ("POST", "/api/devops/load/{command}") => "Control load test",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn router_methods_have_display_names() {
        for file in [
            include_str!("auth/mod.rs"),
            include_str!("papers/mod.rs"),
            include_str!("practice/mod.rs"),
            include_str!("devops.rs"),
        ] {
            for line in file.lines().filter(|l| l.trim().starts_with(".route(")) {
                let route = line.split('"').nth(1).unwrap();
                if !route.starts_with("/api/") { continue; }
                for (verb, method) in [
                    ("get(", "GET"),
                    ("get_route(", "GET"),
                    ("post(", "POST"),
                    ("patch(", "PATCH"),
                    ("put(", "PUT"),
                    ("delete(", "DELETE"),
                ] {
                    if line.contains(verb) {
                        assert!(name(method, route).is_some(), "{method} {route}");
                    }
                }
            }
        }
    }
}
