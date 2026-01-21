//! Admin pages using maud templates.
//!
//! This module contains the admin panel, user management, and excluded domains pages.

use std::collections::HashMap;

use maud::{html, Markup, Render};

use crate::components::{
    Alert, BaseLayout, Button, Form, FormGroup, HiddenInput, Input, ResponsiveTable, StatusBox,
    Table, TableRow, TableVariant,
};
use crate::db::{AuditEvent, ExcludedDomain, ForumAccountLink, User};

/// User status badge for admin panel.
#[derive(Debug, Clone, Copy)]
pub enum UserStatus {
    Deactivated,
    Admin,
    Approved,
    Pending,
}

impl UserStatus {
    /// Determine the status from a user.
    #[must_use]
    pub fn from_user(user: &User) -> Self {
        if !user.is_active {
            Self::Deactivated
        } else if user.is_admin {
            Self::Admin
        } else if user.is_approved {
            Self::Approved
        } else {
            Self::Pending
        }
    }

    /// Get the CSS class for this status badge.
    #[must_use]
    pub const fn css_class(&self) -> &'static str {
        match self {
            Self::Deactivated => "status-badge status-deactivated",
            Self::Admin => "status-badge status-admin",
            Self::Approved => "status-badge status-approved",
            Self::Pending => "status-badge status-pending",
        }
    }

    /// Get the label for this status badge.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Deactivated => "DEACTIVATED",
            Self::Admin => "ADMIN",
            Self::Approved => "APPROVED",
            Self::Pending => "PENDING",
        }
    }
}

/// Render a user status badge.
fn render_user_status_badge(user: &User) -> Markup {
    let status = UserStatus::from_user(user);
    html! {
        span class=(status.css_class()) { (status.label()) }
    }
}

/// Render action buttons for a user in the admin panel.
fn render_user_actions(user: &User, is_current_user: bool) -> Markup {
    // Deactivated users only get reactivate option
    if !user.is_active {
        if is_current_user {
            return html! {};
        }
        return html! {
            (Form::post("/admin/user/reactivate", html! {
                (HiddenInput::new("user_id", &user.id.to_string()))
                (Button::success("Reactivate").r#type("submit").class("btn-sm"))
            }).class("inline-form"))
        };
    }

    html! {
        // Approval/Revoke buttons
        @if !user.is_approved {
            (Form::post("/admin/user/approve", html! {
                (HiddenInput::new("user_id", &user.id.to_string()))
                (Button::success("Approve").r#type("submit").class("btn-sm"))
            }).class("inline-form"))
        } @else if !is_current_user {
            (Form::post("/admin/user/revoke", html! {
                (HiddenInput::new("user_id", &user.id.to_string()))
                (Button::warning("Revoke").r#type("submit").class("btn-sm"))
            }).class("inline-form"))
        }

        // Promote/Demote buttons
        @if !user.is_admin {
            (Form::post("/admin/user/promote", html! {
                (HiddenInput::new("user_id", &user.id.to_string()))
                (Button::primary("Make Admin")
                    .r#type("submit")
                    .class("btn-sm")
                    .onclick("return confirm('Are you sure you want to make this user an admin?')"))
            }).class("inline-form"))
        } @else if !is_current_user {
            (Form::post("/admin/user/demote", html! {
                (HiddenInput::new("user_id", &user.id.to_string()))
                (Button::secondary("Remove Admin")
                    .r#type("submit")
                    .class("btn-sm")
                    .onclick("return confirm('Are you sure you want to remove admin privileges from this user?')"))
            }).class("inline-form"))
        }

        // Deactivate button (not for current user)
        @if !is_current_user {
            (Form::post("/admin/user/deactivate", html! {
                (HiddenInput::new("user_id", &user.id.to_string()))
                (Button::danger("Deactivate")
                    .r#type("submit")
                    .class("btn-sm")
                    .onclick("return confirm('Are you sure you want to deactivate this user?')"))
            }).class("inline-form"))
        }

        // Reset password button
        (Form::post("/admin/user/reset-password", html! {
            (HiddenInput::new("user_id", &user.id.to_string()))
            (Button::secondary("Reset PW")
                .r#type("submit")
                .class("btn-sm")
                .onclick("return confirm('Are you sure you want to reset this user\\'s password? Their current sessions will be invalidated.')"))
        }).class("inline-form"))
    }
}

/// Render a single user row for the admin table.
fn render_user_row(user: &User, current_user: &User) -> Markup {
    let is_current_user = user.id == current_user.id;
    let display_name = user
        .display_name
        .as_ref()
        .filter(|n| !n.is_empty())
        .map_or_else(|| user.username.as_str(), String::as_str);

    let row = TableRow::new()
        .cell(&user.id.to_string())
        .cell_markup(html! {
            a href=(format!("/admin/user/{}", user.id)) {
                code { (user.username) }
            }
        })
        .cell(display_name)
        .cell(user.email.as_deref().unwrap_or("\u{2014}"))
        .cell_markup(render_user_status_badge(user))
        .cell(&user.created_at)
        .cell_markup(render_user_actions(user, is_current_user));

    if is_current_user {
        row.class("current-user").render()
    } else {
        row.render()
    }
}

/// Render the users table for the admin panel.
fn render_users_table(users: &[User], current_user: &User) -> Markup {
    let rows: Vec<Markup> = users
        .iter()
        .map(|u| render_user_row(u, current_user))
        .collect();

    let table = Table::new(vec![
        "ID",
        "Username",
        "Display Name",
        "Email",
        "Status",
        "Created",
        "Actions",
    ])
    .variant(TableVariant::Admin)
    .rows(rows);

    ResponsiveTable::new(table.render()).render()
}

/// Render the target cell for an audit event, making it a link when possible.
fn render_audit_target(event: &AuditEvent) -> Markup {
    match (&event.target_type, event.target_id) {
        (Some(target_type), Some(id)) => {
            let label = format!("{target_type} #{id}");
            match target_type.as_str() {
                "user" => html! {
                    a href=(format!("/admin/user/{}", id)) { (label) }
                },
                "forum_link" => {
                    // Try to extract forum_username from metadata for a better link
                    if let Some(metadata) = &event.metadata {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(metadata) {
                            if let Some(forum_username) =
                                parsed.get("forum_username").and_then(|v| v.as_str())
                            {
                                return html! {
                                    a href=(format!("/admin/forum-user/{}", forum_username)) {
                                        "forum_link: " code { (forum_username) }
                                    }
                                };
                            }
                        }
                    }
                    // Fallback to plain text if we can't extract the username
                    html! { (label) }
                }
                _ => html! { (label) },
            }
        }
        (Some(target_type), None) => {
            // Handle targets without IDs (like excluded_domain)
            match target_type.as_str() {
                "excluded_domain" => html! {
                    a href="/admin/excluded-domains" { "excluded domains" }
                },
                _ => html! { (target_type) },
            }
        }
        _ => html! { "\u{2014}" },
    }
}

/// Render a single audit event row.
fn render_audit_row(event: &AuditEvent, user_lookup: &HashMap<i64, &User>) -> Markup {
    // Determine user display
    let user_str = event.user_id.map_or_else(
        || "System".to_string(),
        |id| {
            if let Some(user) = user_lookup.get(&id) {
                user.display_name
                    .as_ref()
                    .filter(|n| !n.is_empty())
                    .cloned()
                    .unwrap_or_else(|| user.username.clone())
            } else {
                format!("User #{id}")
            }
        },
    );

    html! {
        tr {
            td class="audit-cell" { (event.created_at) }
            td class="audit-cell" {
                @if let Some(user_id) = event.user_id {
                    a href=(format!("/admin/user/{}", user_id)) {
                        (user_str)
                    }
                } @else {
                    (user_str)
                }
            }
            td class="audit-cell" { (event.event_type) }
            td class="audit-cell" { (render_audit_target(event)) }
            td class="audit-cell" { (event.ip_address.as_deref().unwrap_or("\u{2014}")) }
        }
    }
}

/// Render the audit log table.
fn render_audit_table(audit_events: &[AuditEvent], users: &[User]) -> Markup {
    // Build lookup map for user display names
    let user_lookup: HashMap<i64, &User> = users.iter().map(|u| (u.id, u)).collect();

    let rows: Vec<Markup> = audit_events
        .iter()
        .map(|e| render_audit_row(e, &user_lookup))
        .collect();

    let table = Table::new(vec!["Timestamp", "User", "Event", "Target", "IP"])
        .variant(TableVariant::Admin)
        .rows(rows);

    ResponsiveTable::new(table.render()).render()
}

/// Parameters for the admin panel page.
pub struct AdminPanelParams<'a> {
    pub users: &'a [User],
    pub audit_events: &'a [AuditEvent],
    pub forum_links: &'a [ForumAccountLink],
    pub current_user: &'a User,
    /// Optional active tab ("users", "forum-links", "audit")
    pub active_tab: Option<&'a str>,
    /// Optional success/error message
    pub message: Option<&'a str>,
}

/// Render the main admin panel page with tabs.
///
/// # Arguments
///
/// * `params` - Parameters for rendering the admin panel
///
/// # Returns
///
/// Complete HTML page as maud Markup
#[must_use]
pub fn render_admin_panel(params: &AdminPanelParams) -> Markup {
    let active_tab = params.active_tab.unwrap_or("users");

    // Build user lookup for forum links table
    let user_lookup: HashMap<i64, &User> = params.users.iter().map(|u| (u.id, u)).collect();

    let content = html! {
        div class="admin-panel-container" {
            h1 { "Admin Panel" }

            // Optional message
            @if let Some(msg) = params.message {
                (Alert::success(msg).render())
            }

            // Tab navigation
            div class="tab-nav" {
                button class=(if active_tab == "users" { "active" } else { "" })
                    onclick="switchTab('users')" { "Users" }
                button class=(if active_tab == "forum-links" { "active" } else { "" })
                    onclick="switchTab('forum-links')" { "Forum Links" }
                button class=(if active_tab == "audit" { "active" } else { "" })
                    onclick="switchTab('audit')" { "Audit Log" }
            }

            // Users tab
            div id="tab-users" class=(format!("tab-content {}", if active_tab == "users" { "active" } else { "" })) {
                (render_users_table(params.users, params.current_user))

                // Admin tools section
                h3 class="admin-section-header" style="margin-top: var(--spacing-lg);" { "Admin Tools" }
                div class="admin-tools" {
                    (Button::primary("Manage Excluded Domains").href("/admin/excluded-domains"))
                }
            }

            // Forum Links tab
            div id="tab-forum-links" class=(format!("tab-content {}", if active_tab == "forum-links" { "active" } else { "" })) {
                (render_forum_links_section(params.forum_links, &user_lookup))
            }

            // Audit Log tab
            div id="tab-audit" class=(format!("tab-content {}", if active_tab == "audit" { "active" } else { "" })) {
                (render_audit_table(params.audit_events, params.users))
            }
        }

        // Tab switching JavaScript
        script {
            (maud::PreEscaped(r#"
                function switchTab(tabName) {
                    // Hide all tabs
                    document.querySelectorAll('.tab-content').forEach(tab => {
                        tab.classList.remove('active');
                    });
                    // Deactivate all nav buttons
                    document.querySelectorAll('.tab-nav button').forEach(btn => {
                        btn.classList.remove('active');
                    });
                    // Show selected tab
                    document.getElementById('tab-' + tabName).classList.add('active');
                    // Activate selected nav button
                    event.target.classList.add('active');
                    // Update URL without reload
                    const url = new URL(window.location);
                    url.searchParams.set('tab', tabName);
                    window.history.replaceState({}, '', url);
                }
            "#))
        }
    };

    BaseLayout::new("Admin Panel")
        .with_user(Some(params.current_user))
        .render(content)
}

/// Render the forum links section with table and re-archive form.
fn render_forum_links_section(
    forum_links: &[ForumAccountLink],
    user_lookup: &HashMap<i64, &User>,
) -> Markup {
    html! {
        // Re-archive thread form
        div class="add-domain-section" style="margin-bottom: var(--spacing-lg);" {
            h3 { "Re-Archive Forum Thread" }
            p class="page-description" {
                "Submit a forum thread URL to re-archive. This will re-process any "
                code { "link_archive_account" }
                " commands to re-link forum accounts."
            }
            (Form::post("/submit/thread", html! {
                (FormGroup::new(
                    "Thread URL:",
                    "url",
                    Input::text("url")
                        .id("url")
                        .placeholder("https://forum.example.com/t/topic-slug/12345")
                        .required()
                        .render()
                ).render())
                (Button::primary("Re-Archive Thread").r#type("submit"))
            }))
        }

        // Forum links table
        h3 { "Forum Account Links" }
        p class="page-description" {
            "Manage forum account links. Deleting a link will reset the user's display name, allowing them to set a new one or re-link their forum account."
        }
        (render_forum_links_table(forum_links, user_lookup))
    }
}

/// Render a single forum link row.
fn render_forum_link_row(link: &ForumAccountLink, user_lookup: &HashMap<i64, &User>) -> Markup {
    let username = user_lookup
        .get(&link.user_id)
        .map_or_else(|| format!("User #{}", link.user_id), |u| u.username.clone());

    let display_name = user_lookup
        .get(&link.user_id)
        .and_then(|u| u.display_name.as_ref())
        .filter(|n| !n.is_empty())
        .map_or("—", String::as_str);

    let row = TableRow::new()
        .cell(&link.id.to_string())
        .cell_markup(html! {
            a href=(format!("/admin/forum-user/{}", link.forum_username)) {
                code { (&link.forum_username) }
            }
        })
        .cell_markup(html! {
            a href=(format!("/admin/user/{}", link.user_id)) {
                code { (username) }
            }
        })
        .cell(display_name)
        .cell_markup(html! {
            a href=(&link.linked_via_post_url) target="_blank" {
                (link.post_title.as_deref().unwrap_or(&link.linked_via_post_guid))
            }
        })
        .cell(&link.created_at)
        .cell_markup(html! {
            (Form::post("/admin/forum-link/delete", html! {
                (HiddenInput::new("link_id", &link.id.to_string()))
                (Button::danger("Delete")
                    .r#type("submit")
                    .class("btn-sm")
                    .onclick("return confirm('Are you sure you want to delete this forum link? The user\\'s display name will be reset.')"))
            }).class("inline-form"))
        });

    row.render()
}

/// Render the forum links table.
fn render_forum_links_table(
    forum_links: &[ForumAccountLink],
    user_lookup: &HashMap<i64, &User>,
) -> Markup {
    if forum_links.is_empty() {
        return html! {
            p class="no-domains-message" { "No forum account links yet." }
        };
    }

    let rows: Vec<Markup> = forum_links
        .iter()
        .map(|link| render_forum_link_row(link, user_lookup))
        .collect();

    let table = Table::new(vec![
        "ID",
        "Forum Username",
        "Archive User",
        "Display Name",
        "Linked Via",
        "Created",
        "Actions",
    ])
    .variant(TableVariant::Admin)
    .rows(rows);

    ResponsiveTable::new(table.render()).render()
}

/// Render the password reset result page.
///
/// Shows the newly generated password after an admin resets a user's password.
///
/// # Arguments
///
/// * `username` - The username whose password was reset
/// * `new_password` - The newly generated password
///
/// # Returns
///
/// Complete HTML page as maud Markup
#[must_use]
pub fn render_admin_password_reset_result(username: &str, new_password: &str) -> Markup {
    let content = html! {
        div class="password-reset-container" {
            h1 { "Password Reset" }

            (Alert::success(&format!("Password for {} has been reset successfully.", username))
                .render())

            div class="password-display" {
                p class="password-label" { strong { "New Password:" } }
                code class="password-value" { (new_password) }
            }

            (StatusBox::warning(
                "Important",
                "Copy this password now. It will not be shown again. The user's existing sessions have been invalidated."
            ).render())

            div class="action-buttons" {
                (Button::primary("Back to Admin Panel").href("/admin"))
            }
        }
    };

    BaseLayout::new("Password Reset").render(content)
}

/// Render the excluded domain status badge.
fn render_domain_status_badge(is_active: bool) -> Markup {
    if is_active {
        html! {
            span class="domain-status-active" { "Active" }
        }
    } else {
        html! {
            span class="domain-status-inactive" { "Inactive" }
        }
    }
}

/// Render actions for an excluded domain row.
fn render_domain_actions(domain: &ExcludedDomain) -> Markup {
    let toggle_action = if domain.is_active {
        "Disable"
    } else {
        "Enable"
    };

    html! {
        (Form::post("/admin/excluded-domains/toggle", html! {
            (HiddenInput::new("domain", &domain.domain))
            (Button::secondary(toggle_action).r#type("submit").class("btn-sm"))
        }).class("inline-form"))

        (Form::post("/admin/excluded-domains/delete", html! {
            (HiddenInput::new("domain", &domain.domain))
            (Button::danger("Delete")
                .r#type("submit")
                .class("btn-sm")
                .onclick("return confirm('Are you sure you want to delete this domain?');"))
        }).class("inline-form"))
    }
}

/// Render a single excluded domain row.
fn render_domain_row(domain: &ExcludedDomain) -> Markup {
    let row = TableRow::new()
        .cell_markup(html! { code { (domain.domain) } })
        .cell(&domain.reason)
        .cell_markup(render_domain_status_badge(domain.is_active))
        .cell(&domain.created_at)
        .cell_markup(render_domain_actions(domain));

    row.render()
}

/// Render the excluded domains table.
fn render_excluded_domains_table(domains: &[ExcludedDomain]) -> Markup {
    if domains.is_empty() {
        return html! {
            p class="no-domains-message" { "No excluded domains yet." }
        };
    }

    let rows: Vec<Markup> = domains.iter().map(render_domain_row).collect();

    let table = Table::new(vec!["Domain", "Reason", "Status", "Added", "Actions"]).rows(rows);

    ResponsiveTable::new(table.render()).render()
}

/// Render the excluded domains management page.
///
/// # Arguments
///
/// * `domains` - List of excluded domains
/// * `message` - Optional success/error message to display
///
/// # Returns
///
/// Complete HTML page as maud Markup
#[must_use]
pub fn render_admin_excluded_domains_page(
    domains: &[ExcludedDomain],
    message: Option<&str>,
) -> Markup {
    let content = html! {
        div class="excluded-domains-container" {
            h1 { "Excluded Domains" }

            p class="page-description" {
                "Manage domains that should not be archived. These domains will be automatically excluded from archiving."
            }

            // Success/error message if present
            @if let Some(msg) = message {
                (Alert::success(msg).render())
            }

            // Add new domain form
            div class="add-domain-section" {
                h2 { "Add New Excluded Domain" }
                (Form::post("/admin/excluded-domains/add", html! {
                    (FormGroup::new(
                        "Domain:",
                        "domain",
                        Input::text("domain")
                            .id("domain")
                            .placeholder("example.com")
                            .required()
                            .render()
                    ).render())

                    (FormGroup::new(
                        "Reason (optional):",
                        "reason",
                        Input::text("reason")
                            .id("reason")
                            .placeholder("Self-hosted instance")
                            .render()
                    ).render())

                    (Button::primary("Add Domain").r#type("submit"))
                }))
            }

            // Existing domains table
            div class="domains-list-section" {
                h2 { "Current Excluded Domains" }
                (render_excluded_domains_table(domains))
            }

            // Back button
            div class="action-buttons" {
                (Button::outline("Back to Admin Panel").href("/admin"))
            }
        }
    };

    BaseLayout::new("Excluded Domains").render(content)
}

/// Render the admin user profile page.
///
/// # Arguments
///
/// * `user` - The user to display
/// * `forum_link` - Optional forum account link
/// * `audit_events` - Recent audit events for this user
/// * `current_user` - The currently logged-in admin user
///
/// # Returns
///
/// Complete HTML page as maud Markup
#[must_use]
pub fn render_admin_user_profile(
    user: &User,
    forum_link: Option<&ForumAccountLink>,
    audit_events: &[AuditEvent],
    current_user: &User,
) -> Markup {
    let display_name = user
        .display_name
        .as_ref()
        .filter(|n| !n.is_empty())
        .map_or_else(|| user.username.as_str(), String::as_str);

    let content = html! {
        div class="user-profile-container" {
            h1 { "User Profile" }

            // User info section
            div class="user-info-section" {
                h2 { "User Details" }
                table class="user-details-table" {
                    tr {
                        th { "ID:" }
                        td { (user.id) }
                    }
                    tr {
                        th { "Username:" }
                        td { code { (user.username) } }
                    }
                    tr {
                        th { "Display Name:" }
                        td { (display_name) }
                    }
                    tr {
                        th { "Email:" }
                        td { (user.email.as_deref().unwrap_or("\u{2014}")) }
                    }
                    tr {
                        th { "Status:" }
                        td { (render_user_status_badge(user)) }
                    }
                    tr {
                        th { "Created:" }
                        td { (user.created_at) }
                    }
                    tr {
                        th { "Updated:" }
                        td { (user.updated_at) }
                    }
                    tr {
                        th { "Password Updated:" }
                        td { (user.password_updated_at) }
                    }
                    @if let Some(locked_until) = &user.locked_until {
                        tr {
                            th { "Locked Until:" }
                            td { (locked_until) }
                        }
                    }
                    tr {
                        th { "Failed Login Attempts:" }
                        td { (user.failed_login_attempts) }
                    }
                }
            }

            // Forum account link section
            @if let Some(link) = forum_link {
                div class="forum-link-section" {
                    h2 { "Forum Account Link" }
                    table class="user-details-table" {
                        tr {
                            th { "Forum Username:" }
                            td {
                                a href=(format!("/admin/forum-user/{}", link.forum_username)) {
                                    code { (link.forum_username) }
                                }
                            }
                        }
                        tr {
                            th { "Linked Via Post:" }
                            td {
                                a href=(&link.linked_via_post_url) target="_blank" { (&link.linked_via_post_guid) }
                            }
                        }
                        @if let Some(title) = &link.post_title {
                            tr {
                                th { "Post Title:" }
                                td { (title) }
                            }
                        }
                        tr {
                            th { "Linked At:" }
                            td { (link.created_at) }
                        }
                    }
                }
            }

            // Audit log section
            @if !audit_events.is_empty() {
                div class="audit-log-section" {
                    h2 { "Recent Activity" }
                    (render_user_audit_table(audit_events))
                }
            }

            // Back button
            div class="action-buttons" {
                (Button::outline("Back to Admin Panel").href("/admin"))
            }
        }
    };

    BaseLayout::new(&format!("User: {}", user.username))
        .with_user(Some(current_user))
        .render(content)
}

/// Render the admin forum user profile page.
///
/// # Arguments
///
/// * `forum_link` - The forum account link
/// * `user` - Optional linked user account
/// * `current_user` - The currently logged-in admin user
///
/// # Returns
///
/// Complete HTML page as maud Markup
#[must_use]
pub fn render_admin_forum_user_profile(
    forum_link: &ForumAccountLink,
    user: Option<&User>,
    current_user: &User,
) -> Markup {
    let content = html! {
        div class="forum-user-profile-container" {
            h1 { "Forum User Profile" }

            // Forum user info section
            div class="forum-user-info-section" {
                h2 { "Forum Account Details" }
                table class="user-details-table" {
                    tr {
                        th { "Forum Username:" }
                        td { code { (forum_link.forum_username) } }
                    }
                    tr {
                        th { "Linked Via Post:" }
                        td {
                            a href=(&forum_link.linked_via_post_url) target="_blank" { (&forum_link.linked_via_post_guid) }
                        }
                    }
                    @if let Some(title) = &forum_link.post_title {
                        tr {
                            th { "Post Title:" }
                            td { (title) }
                        }
                    }
                    @if let Some(author_raw) = &forum_link.forum_author_raw {
                        tr {
                            th { "Author Raw:" }
                            td { (author_raw) }
                        }
                    }
                    tr {
                        th { "Linked At:" }
                        td { (forum_link.created_at) }
                    }
                }
            }

            // Linked user section
            @if let Some(linked_user) = user {
                div class="linked-user-section" {
                    h2 { "Linked User Account" }
                    table class="user-details-table" {
                        tr {
                            th { "User ID:" }
                            td { (linked_user.id) }
                        }
                        tr {
                            th { "Username:" }
                            td { code { (&linked_user.username) } }
                        }
                        tr {
                            th { "Display Name:" }
                            td {
                                @if let Some(ref dn) = linked_user.display_name {
                                    @if !dn.is_empty() {
                                        (dn)
                                    } @else {
                                        em class="text-muted" { "(not set)" }
                                    }
                                } @else {
                                    em class="text-muted" { "(not set)" }
                                }
                            }
                        }
                        tr {
                            th { "Status:" }
                            td { (render_user_status_badge(linked_user)) }
                        }
                    }

                    div class="action-buttons" style="display: flex; gap: var(--spacing-sm); align-items: center; margin-top: var(--spacing-md);" {
                        (Button::primary("View User Profile")
                            .href(&format!("/admin/user/{}", linked_user.id)))

                        (Form::post("/admin/forum-link/delete", html! {
                            (HiddenInput::new("link_id", &forum_link.id.to_string()))
                            (Button::danger("Remove Link & Reset Display Name")
                                .r#type("submit")
                                .onclick("return confirm('Are you sure you want to remove this forum link? The user\\'s display name will be reset to allow them to set a new one.')"))
                        }).class("inline-form"))
                    }
                }
            } @else {
                div class="no-link-section" {
                    (StatusBox::warning(
                        "No Link",
                        "This forum user is not linked to any user account."
                    ).render())
                }
            }

            // Back button
            div class="action-buttons" {
                (Button::outline("Back to Admin Panel").href("/admin"))
            }
        }
    };

    BaseLayout::new(&format!("Forum User: {}", forum_link.forum_username))
        .with_user(Some(current_user))
        .render(content)
}

/// Render the audit log table for user profile.
fn render_user_audit_table(audit_events: &[AuditEvent]) -> Markup {
    let rows: Vec<Markup> = audit_events
        .iter()
        .map(|event| {
            html! {
                tr {
                    td class="audit-cell" { (event.created_at) }
                    td class="audit-cell" { (event.event_type) }
                    td class="audit-cell" { (render_audit_target(event)) }
                    td class="audit-cell" { (event.ip_address.as_deref().unwrap_or("\u{2014}")) }
                }
            }
        })
        .collect();

    let table = Table::new(vec!["Timestamp", "Event", "Target", "IP"])
        .variant(TableVariant::Admin)
        .rows(rows);

    ResponsiveTable::new(table.render()).render()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a test user for unit tests.
    fn test_user(
        id: i64,
        username: &str,
        is_admin: bool,
        is_approved: bool,
        is_active: bool,
    ) -> User {
        User {
            id,
            username: username.to_string(),
            password_hash: "hash".to_string(),
            email: Some(format!("{}@example.com", username)),
            display_name: Some(format!("{} Display", username)),
            is_approved,
            is_admin,
            is_active,
            failed_login_attempts: 0,
            locked_until: None,
            password_updated_at: "2024-01-01T00:00:00Z".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    fn test_audit_event(id: i64, user_id: Option<i64>, event_type: &str) -> AuditEvent {
        AuditEvent {
            id,
            user_id,
            event_type: event_type.to_string(),
            target_type: Some("user".to_string()),
            target_id: Some(2),
            metadata: None,
            ip_address: Some("192.168.1.1".to_string()),
            forwarded_for: None,
            user_agent: None,
            user_agent_id: None,
            created_at: "2024-01-01T12:00:00Z".to_string(),
        }
    }

    fn test_excluded_domain(id: i64, domain: &str, is_active: bool) -> ExcludedDomain {
        ExcludedDomain {
            id,
            domain: domain.to_string(),
            reason: "Test reason".to_string(),
            is_active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            created_by_user_id: Some(1),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_user_status_from_user() {
        let deactivated = test_user(1, "deactivated", false, false, false);
        assert!(matches!(
            UserStatus::from_user(&deactivated),
            UserStatus::Deactivated
        ));

        let admin = test_user(2, "admin", true, true, true);
        assert!(matches!(UserStatus::from_user(&admin), UserStatus::Admin));

        let approved = test_user(3, "approved", false, true, true);
        assert!(matches!(
            UserStatus::from_user(&approved),
            UserStatus::Approved
        ));

        let pending = test_user(4, "pending", false, false, true);
        assert!(matches!(
            UserStatus::from_user(&pending),
            UserStatus::Pending
        ));
    }

    #[test]
    fn test_user_status_css_classes() {
        assert_eq!(
            UserStatus::Deactivated.css_class(),
            "status-badge status-deactivated"
        );
        assert_eq!(UserStatus::Admin.css_class(), "status-badge status-admin");
        assert_eq!(
            UserStatus::Approved.css_class(),
            "status-badge status-approved"
        );
        assert_eq!(
            UserStatus::Pending.css_class(),
            "status-badge status-pending"
        );
    }

    #[test]
    fn test_render_user_status_badge() {
        let admin = test_user(1, "admin", true, true, true);
        let html = render_user_status_badge(&admin).into_string();
        assert!(html.contains("status-admin"));
        assert!(html.contains("ADMIN"));

        let pending = test_user(2, "pending", false, false, true);
        let html = render_user_status_badge(&pending).into_string();
        assert!(html.contains("status-pending"));
        assert!(html.contains("PENDING"));
    }

    #[test]
    fn test_render_user_actions_current_user() {
        let user = test_user(1, "current", true, true, true);
        let html = render_user_actions(&user, true).into_string();

        // Current user should not see deactivate or demote buttons
        assert!(!html.contains("Deactivate"));
        assert!(!html.contains("Remove Admin"));
        // But should still see reset password
        assert!(html.contains("Reset PW"));
    }

    #[test]
    fn test_render_user_actions_other_user() {
        let user = test_user(2, "other", false, true, true);
        let html = render_user_actions(&user, false).into_string();

        // Should see all applicable buttons
        assert!(html.contains("Revoke"));
        assert!(html.contains("Make Admin"));
        assert!(html.contains("Deactivate"));
        assert!(html.contains("Reset PW"));
    }

    #[test]
    fn test_render_user_actions_deactivated_user() {
        let user = test_user(3, "deactivated", false, false, false);
        let html = render_user_actions(&user, false).into_string();

        // Deactivated user should only show reactivate
        assert!(html.contains("Reactivate"));
        assert!(!html.contains("Deactivate"));
        assert!(!html.contains("Make Admin"));
    }

    fn test_forum_link(id: i64, user_id: i64, forum_username: &str) -> ForumAccountLink {
        ForumAccountLink {
            id,
            user_id,
            forum_username: forum_username.to_string(),
            linked_via_post_guid: "test-guid".to_string(),
            linked_via_post_url: "https://example.com/post/1".to_string(),
            forum_author_raw: Some(format!("@{forum_username}")),
            post_title: Some("Test Post".to_string()),
            post_published_at: Some("2024-01-01T00:00:00Z".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_render_admin_panel() {
        let admin = test_user(1, "admin", true, true, true);
        let user = test_user(2, "testuser", false, true, true);
        let users = vec![admin.clone(), user];
        let events = vec![test_audit_event(1, Some(1), "login")];
        let forum_links = vec![test_forum_link(1, 2, "forumuser")];

        let params = AdminPanelParams {
            users: &users,
            audit_events: &events,
            forum_links: &forum_links,
            current_user: &admin,
            active_tab: None,
            message: None,
        };
        let html = render_admin_panel(&params).into_string();

        // Check page structure
        assert!(html.contains("Admin Panel"));
        assert!(html.contains("Users"));
        assert!(html.contains("Forum Links"));
        assert!(html.contains("Audit Log"));

        // Check user table content
        assert!(html.contains("admin"));
        assert!(html.contains("testuser"));

        // Check audit log content
        assert!(html.contains("login"));
        assert!(html.contains("192.168.1.1"));

        // Check admin tools link
        assert!(html.contains("Manage Excluded Domains"));
        assert!(html.contains("/admin/excluded-domains"));

        // Check forum links
        assert!(html.contains("forumuser"));
    }

    #[test]
    fn test_render_admin_panel_with_message() {
        let admin = test_user(1, "admin", true, true, true);
        let params = AdminPanelParams {
            users: &[admin.clone()],
            audit_events: &[],
            forum_links: &[],
            current_user: &admin,
            active_tab: Some("forum-links"),
            message: Some("Test message"),
        };
        let html = render_admin_panel(&params).into_string();

        assert!(html.contains("Test message"));
        // Forum links tab should be active
        assert!(html.contains("tab-forum-links"));
    }

    #[test]
    fn test_render_forum_links_table_empty() {
        let user_lookup: HashMap<i64, &User> = HashMap::new();
        let html = render_forum_links_table(&[], &user_lookup).into_string();
        assert!(html.contains("No forum account links yet"));
    }

    #[test]
    fn test_render_admin_password_reset_result() {
        let html = render_admin_password_reset_result("testuser", "newpassword123").into_string();

        assert!(html.contains("Password Reset"));
        assert!(html.contains("testuser"));
        assert!(html.contains("newpassword123"));
        assert!(html.contains("Important"));
        assert!(html.contains("Copy this password now"));
        assert!(html.contains("Back to Admin Panel"));
    }

    #[test]
    fn test_render_domain_status_badge() {
        let active = render_domain_status_badge(true).into_string();
        assert!(active.contains("domain-status-active"));
        assert!(active.contains("Active"));

        let inactive = render_domain_status_badge(false).into_string();
        assert!(inactive.contains("domain-status-inactive"));
        assert!(inactive.contains("Inactive"));
    }

    #[test]
    fn test_render_excluded_domains_table_empty() {
        let html = render_excluded_domains_table(&[]).into_string();
        assert!(html.contains("No excluded domains yet"));
    }

    #[test]
    fn test_render_excluded_domains_table_with_domains() {
        let domains = vec![
            test_excluded_domain(1, "example.com", true),
            test_excluded_domain(2, "test.org", false),
        ];
        let html = render_excluded_domains_table(&domains).into_string();

        assert!(html.contains("example.com"));
        assert!(html.contains("test.org"));
        assert!(html.contains("Enable"));
        assert!(html.contains("Disable"));
        assert!(html.contains("Delete"));
    }

    #[test]
    fn test_render_admin_excluded_domains_page() {
        let domains = vec![test_excluded_domain(1, "example.com", true)];
        let html = render_admin_excluded_domains_page(&domains, Some("Domain added successfully!"))
            .into_string();

        assert!(html.contains("Excluded Domains"));
        assert!(html.contains("Add New Excluded Domain"));
        assert!(html.contains("Current Excluded Domains"));
        assert!(html.contains("example.com"));
        assert!(html.contains("Domain added successfully!"));
        assert!(html.contains("Back to Admin Panel"));
    }

    #[test]
    fn test_render_admin_excluded_domains_page_no_message() {
        let html = render_admin_excluded_domains_page(&[], None).into_string();

        assert!(html.contains("Excluded Domains"));
        assert!(html.contains("No excluded domains yet"));
        // Should not contain any alert
        assert!(!html.contains("class=\"success\""));
    }

    #[test]
    fn test_audit_row_with_user() {
        let user = test_user(1, "admin", true, true, true);
        let users = vec![user];
        let user_lookup: HashMap<i64, &User> = users.iter().map(|u| (u.id, u)).collect();

        let event = test_audit_event(1, Some(1), "user_approved");
        let html = render_audit_row(&event, &user_lookup).into_string();

        assert!(html.contains("admin Display")); // Display name
        assert!(html.contains("user_approved"));
        assert!(html.contains("192.168.1.1"));
    }

    #[test]
    fn test_audit_row_system() {
        let event = AuditEvent {
            id: 1,
            user_id: None,
            event_type: "system_startup".to_string(),
            target_type: None,
            target_id: None,
            metadata: None,
            ip_address: None,
            forwarded_for: None,
            user_agent: None,
            user_agent_id: None,
            created_at: "2024-01-01T12:00:00Z".to_string(),
        };

        let user_lookup: HashMap<i64, &User> = HashMap::new();
        let html = render_audit_row(&event, &user_lookup).into_string();

        assert!(html.contains("System"));
        assert!(html.contains("system_startup"));
    }
}
