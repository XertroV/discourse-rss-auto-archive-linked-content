//! Submit page templates using maud.
//!
//! This module provides maud-based templates for the URL submission pages:
//! - Submit form with tabs for single URL and thread archive
//! - Success message after submission
//! - Error message for failed submissions

use maud::{html, Markup, Render};

use crate::components::{Alert, BaseLayout, Button, ContentTabs, Form, FormHelp, Input, Label};
use crate::db::User;

/// Parameters for rendering the submit form page.
#[derive(Debug, Clone, Default)]
pub struct SubmitFormParams<'a> {
    /// Optional error message to display.
    pub error: Option<&'a str>,
    /// Optional success message to display.
    pub success: Option<&'a str>,
    /// Optional authentication warning (e.g., "Login required for submissions").
    pub auth_warning: Option<&'a str>,
    /// Whether the user can submit (affects disabled state of form elements).
    pub can_submit: bool,
    /// Optional authenticated user for header navigation.
    pub user: Option<&'a User>,
}

impl<'a> SubmitFormParams<'a> {
    /// Create new submit form params with default values.
    #[must_use]
    pub fn new() -> Self {
        Self {
            can_submit: true,
            ..Default::default()
        }
    }

    /// Set the error message.
    #[must_use]
    pub fn error(mut self, error: &'a str) -> Self {
        self.error = Some(error);
        self
    }

    /// Set the success message.
    #[must_use]
    pub fn success(mut self, success: &'a str) -> Self {
        self.success = Some(success);
        self
    }

    /// Set the authentication warning.
    #[must_use]
    pub fn auth_warning(mut self, warning: &'a str) -> Self {
        self.auth_warning = Some(warning);
        self
    }

    /// Set whether submissions are allowed.
    #[must_use]
    pub fn can_submit(mut self, can_submit: bool) -> Self {
        self.can_submit = can_submit;
        self
    }

    /// Set the authenticated user.
    #[must_use]
    pub fn user(mut self, user: &'a User) -> Self {
        self.user = Some(user);
        self
    }

    /// Set the authenticated user from an Option.
    #[must_use]
    pub fn with_user(mut self, user: Option<&'a User>) -> Self {
        self.user = user;
        self
    }
}

/// Render the submit form page with tabs for URL and thread submission.
///
/// # Arguments
///
/// * `params` - Parameters controlling the form display
///
/// # Example
///
/// ```ignore
/// use crate::web::pages::submit::{render_submit_form_page, SubmitFormParams};
///
/// let params = SubmitFormParams::new()
///     .can_submit(true)
///     .auth_warning("Login required");
/// let html = render_submit_form_page(&params);
/// ```
#[must_use]
pub fn render_submit_form_page(params: &SubmitFormParams<'_>) -> Markup {
    let content = html! {
        h1 { "Submit for Archiving" }

        // Auth warning (if any)
        @if let Some(warning) = params.auth_warning {
            (AuthWarning::new(warning))
        }

        // Error message (if any)
        @if let Some(err) = params.error {
            (Alert::error(err).with_title("Error"))
        }

        // Success message (if any)
        @if let Some(msg) = params.success {
            (Alert::success(msg).with_title("Success"))
        }

        // Tabs component with URL and Thread archive options
        (ContentTabs::new()
            .tab(
                "url-tab",
                "Single URL",
                html! { (UrlSubmissionTab { can_submit: params.can_submit }) },
                true
            )
            .tab(
                "thread-tab",
                "Archive Thread",
                html! { (ThreadArchiveTab { can_submit: params.can_submit }) },
                false
            ))
    };

    BaseLayout::new("Submit for Archiving", params.user).render(content)
}

/// Render the submit form page using the old-style function signature.
///
/// This function matches the signature of the original `render_submit_form`
/// for backwards compatibility during migration.
#[must_use]
pub fn render_submit_form(
    error: Option<&str>,
    success: Option<&str>,
    auth_warning: Option<&str>,
    can_submit: bool,
) -> String {
    let params = SubmitFormParams {
        error,
        success,
        auth_warning,
        can_submit,
        user: None,
    };
    render_submit_form_page(&params).into_string()
}

/// Render the submission success page.
///
/// Displays a success message with the submission ID and a link to submit another URL.
///
/// # Arguments
///
/// * `submission_id` - The ID of the created submission
/// * `user` - Optional authenticated user for header navigation
///
/// # Example
///
/// ```ignore
/// use crate::web::pages::submit::render_submit_success_page;
///
/// let html = render_submit_success_page(12345, None);
/// ```
#[must_use]
pub fn render_submit_success_page(submission_id: i64, user: Option<&User>) -> Markup {
    let content = html! {
        h1 { "URL Submitted Successfully" }

        article class="success" {
            p { "Your URL has been queued for archiving." }
            p {
                strong { "Submission ID:" }
                " "
                (submission_id)
            }
            p { "The archive will be processed shortly. Check back later for results." }
        }

        p {
            a href="/submit" { "Submit another URL" }
        }
    };

    BaseLayout::new("Submission Successful", user).render(content)
}

/// Render the submission success page (backwards compatible).
///
/// This function matches the signature of the original `render_submit_success`
/// for backwards compatibility during migration.
#[must_use]
pub fn render_submit_success(submission_id: i64) -> String {
    render_submit_success_page(submission_id, None).into_string()
}

/// Render the submission error page.
///
/// Displays an error message and a link to try again.
///
/// # Arguments
///
/// * `error` - The error message to display
/// * `user` - Optional authenticated user for header navigation
///
/// # Example
///
/// ```ignore
/// use crate::web::pages::submit::render_submit_error_page;
///
/// let html = render_submit_error_page("Invalid URL format", None);
/// ```
#[must_use]
pub fn render_submit_error_page(error: &str, user: Option<&User>) -> Markup {
    let content = html! {
        h1 { "Submission Failed" }

        article class="error" {
            p {
                strong { "Error:" }
                " "
                (error)
            }
        }

        p {
            a href="/submit" { "Try again" }
        }
    };

    BaseLayout::new("Submission Failed", user).render(content)
}

/// Render the submission error page (backwards compatible).
///
/// This function matches the signature of the original `render_submit_error`
/// for backwards compatibility during migration.
#[must_use]
pub fn render_submit_error(error: &str) -> String {
    render_submit_error_page(error, None).into_string()
}

/// Authentication warning component.
///
/// Displays a warning message about authentication requirements.
struct AuthWarning<'a> {
    message: &'a str,
}

impl<'a> AuthWarning<'a> {
    fn new(message: &'a str) -> Self {
        Self { message }
    }
}

impl Render for AuthWarning<'_> {
    fn render(&self) -> Markup {
        html! {
            article style="background: var(--warning-bg, #fef3c7); border: 1px solid var(--warning-border, #fcd34d); padding: var(--spacing-md, 1rem); margin-bottom: var(--spacing-md, 1rem); border-radius: var(--radius, 0.375rem);" {
                p style="margin: 0; color: var(--warning-text, #92400e);" {
                    strong { "Note:" }
                    " "
                    (self.message)
                }
            }
        }
    }
}

/// Single URL submission tab content.
struct UrlSubmissionTab {
    can_submit: bool,
}

impl Render for UrlSubmissionTab {
    fn render(&self) -> Markup {
        let submit_button = if self.can_submit {
            Button::primary("Submit for Archiving").r#type("submit")
        } else {
            Button::primary("Submit for Archiving")
                .r#type("submit")
                .disabled()
        };

        let url_input = if self.can_submit {
            Input::url("url")
                .id("url")
                .required()
                .placeholder("https://reddit.com/r/...")
                .pattern("https?://.*")
        } else {
            Input::url("url")
                .id("url")
                .required()
                .placeholder("https://reddit.com/r/...")
                .pattern("https?://.*")
                .disabled()
        };

        let form_content = html! {
            (Label::new("url", "URL to Archive"))
            (url_input)
            (FormHelp::new("Enter the full URL including https://"))
            (submit_button)
        };

        html! {
            article {
                p {
                    "Submit a URL to be archived. Supported sites include Reddit, Twitter/X, "
                    "TikTok, YouTube, Instagram, Imgur, and more."
                }
                p {
                    strong { "Rate limit:" }
                    " 60 submissions per hour per IP address."
                }
            }
            (Form::post("/submit", form_content))
        }
    }
}

/// Thread archive tab content.
struct ThreadArchiveTab {
    can_submit: bool,
}

impl Render for ThreadArchiveTab {
    fn render(&self) -> Markup {
        let submit_button = if self.can_submit {
            Button::primary("Archive Thread").r#type("submit")
        } else {
            Button::primary("Archive Thread")
                .r#type("submit")
                .disabled()
        };

        let thread_input = if self.can_submit {
            Input::url("thread_url")
                .id("thread_url")
                .required()
                .placeholder("https://discuss.example.com/t/topic-name/123")
                .pattern("https?://.*")
        } else {
            Input::url("thread_url")
                .id("thread_url")
                .required()
                .placeholder("https://discuss.example.com/t/topic-name/123")
                .pattern("https?://.*")
                .disabled()
        };

        let form_content = html! {
            (Label::new("thread_url", "Thread URL"))
            (thread_input)
            (FormHelp::new("Paste the URL of a Discourse thread"))
            (submit_button)
        };

        html! {
            article {
                p {
                    "Archive all links from all posts in a Discourse thread. "
                    "The system will fetch the thread's RSS feed and process each post."
                }
                p {
                    strong { "Rate limit:" }
                    " 5 thread archives per hour."
                }
            }
            (Form::post("/submit/thread", form_content))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_submit_form_basic() {
        let params = SubmitFormParams::new();
        let html = render_submit_form_page(&params).into_string();

        // Check page structure
        assert!(html.contains("<h1>Submit for Archiving</h1>"));
        assert!(html.contains("tab-nav"));
        assert!(html.contains("Single URL"));
        assert!(html.contains("Archive Thread"));

        // Check URL tab
        assert!(html.contains("id=\"url-tab\""));
        assert!(html.contains("URL to Archive"));
        assert!(html.contains("action=\"/submit\""));

        // Check thread tab
        assert!(html.contains("id=\"thread-tab\""));
        assert!(html.contains("Thread URL"));
        assert!(html.contains("action=\"/submit/thread\""));
    }

    #[test]
    fn test_render_submit_form_with_error() {
        let params = SubmitFormParams::new().error("Invalid URL format");
        let html = render_submit_form_page(&params).into_string();

        assert!(html.contains("class=\"error\""));
        assert!(html.contains("Invalid URL format"));
        assert!(html.contains("<strong>Error</strong>"));
    }

    #[test]
    fn test_render_submit_form_with_success() {
        let params = SubmitFormParams::new().success("URL queued successfully");
        let html = render_submit_form_page(&params).into_string();

        assert!(html.contains("class=\"success\""));
        assert!(html.contains("URL queued successfully"));
        assert!(html.contains("<strong>Success</strong>"));
    }

    #[test]
    fn test_render_submit_form_with_auth_warning() {
        let params = SubmitFormParams::new().auth_warning("Login required to submit URLs");
        let html = render_submit_form_page(&params).into_string();

        assert!(html.contains("Login required to submit URLs"));
        assert!(html.contains("<strong>Note:</strong>"));
        assert!(html.contains("var(--warning-bg"));
    }

    #[test]
    fn test_render_submit_form_disabled() {
        let params = SubmitFormParams::new().can_submit(false);
        let html = render_submit_form_page(&params).into_string();

        // Should contain disabled inputs and buttons
        assert!(html.contains("disabled"));
    }

    #[test]
    fn test_render_submit_form_enabled() {
        let params = SubmitFormParams::new().can_submit(true);
        let html = render_submit_form_page(&params).into_string();

        // Check that form elements are not disabled
        // The URL input should be enabled
        assert!(html.contains(r#"id="url""#));
        assert!(html.contains(r#"type="url""#));
    }

    #[test]
    fn test_render_submit_success_page() {
        let html = render_submit_success_page(12345, None).into_string();

        assert!(html.contains("<h1>URL Submitted Successfully</h1>"));
        assert!(html.contains("class=\"success\""));
        assert!(html.contains("12345"));
        assert!(html.contains("Submission ID"));
        assert!(html.contains("queued for archiving"));
        assert!(html.contains("href=\"/submit\""));
        assert!(html.contains("Submit another URL"));
    }

    #[test]
    fn test_render_submit_error_page() {
        let html = render_submit_error_page("Rate limit exceeded", None).into_string();

        assert!(html.contains("<h1>Submission Failed</h1>"));
        assert!(html.contains("class=\"error\""));
        assert!(html.contains("Rate limit exceeded"));
        assert!(html.contains("<strong>Error:</strong>"));
        assert!(html.contains("href=\"/submit\""));
        assert!(html.contains("Try again"));
    }

    #[test]
    fn test_backwards_compatible_render_submit_form() {
        let html = render_submit_form(Some("Test error"), None, Some("Auth warning"), true);

        assert!(html.contains("Test error"));
        assert!(html.contains("Auth warning"));
        assert!(html.contains("Submit for Archiving"));
    }

    #[test]
    fn test_backwards_compatible_render_submit_success() {
        let html = render_submit_success(999);

        assert!(html.contains("999"));
        assert!(html.contains("URL Submitted Successfully"));
    }

    #[test]
    fn test_backwards_compatible_render_submit_error() {
        let html = render_submit_error("Something went wrong");

        assert!(html.contains("Something went wrong"));
        assert!(html.contains("Submission Failed"));
    }

    #[test]
    fn test_tab_structure_rendered() {
        let params = SubmitFormParams::new();
        let html = render_submit_form_page(&params).into_string();

        // Check that tab navigation structure is rendered
        // Note: CSS styles are now in static/css/style.css, not inline
        assert!(html.contains("class=\"tab-nav\""));
        assert!(html.contains("class=\"tab-content active\""));
        assert!(html.contains("id=\"url-tab\""));
        assert!(html.contains("id=\"thread-tab\""));
    }

    #[test]
    fn test_tab_script_included() {
        let params = SubmitFormParams::new();
        let html = render_submit_form_page(&params).into_string();

        // Check that tab switching script is embedded
        assert!(html.contains("function showTab"));
        assert!(html.contains("classList.remove('active')"));
        assert!(html.contains("classList.add('active')"));
    }

    #[test]
    fn test_url_tab_content() {
        let tab = UrlSubmissionTab { can_submit: true };
        let html = tab.render().into_string();

        assert!(html.contains("Reddit, Twitter/X"));
        assert!(html.contains("TikTok, YouTube, Instagram, Imgur"));
        assert!(html.contains("60 submissions per hour"));
        assert!(html.contains(r#"name="url""#));
        assert!(html.contains(r#"placeholder="https://reddit.com/r/...""#));
    }

    #[test]
    fn test_thread_tab_content() {
        let tab = ThreadArchiveTab { can_submit: true };
        let html = tab.render().into_string();

        assert!(html.contains("Archive all links from all posts"));
        assert!(html.contains("Discourse thread"));
        assert!(html.contains("5 thread archives per hour"));
        assert!(html.contains(r#"name="thread_url""#));
        assert!(html.contains("https://discuss.example.com/t/topic-name/123"));
    }

    #[test]
    fn test_auth_warning_render() {
        let warning = AuthWarning::new("Please login first");
        let html = warning.render().into_string();

        assert!(html.contains("<strong>Note:</strong>"));
        assert!(html.contains("Please login first"));
        assert!(html.contains("var(--warning-bg"));
        assert!(html.contains("var(--warning-border"));
    }

    #[test]
    fn test_submit_form_with_user() {
        let user = User {
            id: 1,
            username: "testuser".to_string(),
            password_hash: "hash".to_string(),
            email: Some("test@example.com".to_string()),
            display_name: Some("Test User".to_string()),
            is_approved: true,
            is_admin: false,
            is_active: true,
            failed_login_attempts: 0,
            locked_until: None,
            password_updated_at: "2024-01-01T00:00:00Z".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let params = SubmitFormParams::new().user(&user);
        let html = render_submit_form_page(&params).into_string();

        // When user is provided, should show profile link instead of login
        assert!(html.contains(r#"href="/profile""#));
        assert!(!html.contains(r#"href="/login">Login</a>"#));
    }

    #[test]
    fn test_submit_params_builder() {
        let params = SubmitFormParams::new()
            .error("error msg")
            .success("success msg")
            .auth_warning("warning msg")
            .can_submit(false);

        assert_eq!(params.error, Some("error msg"));
        assert_eq!(params.success, Some("success msg"));
        assert_eq!(params.auth_warning, Some("warning msg"));
        assert!(!params.can_submit);
    }
}
