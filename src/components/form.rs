//! Form components for maud templates.
//!
//! This module provides reusable form components that match the styles
//! defined in `static/css/style.css`.

use maud::{html, Markup, Render};

/// A form container element.
#[derive(Debug)]
pub struct Form<'a> {
    /// Form action URL
    pub action: &'a str,
    /// HTTP method ("get" or "post")
    pub method: &'a str,
    /// Form content (inputs, buttons, etc.)
    pub content: Markup,
    /// Optional CSS class
    pub class: Option<&'a str>,
    /// Optional form ID
    pub id: Option<&'a str>,
    /// Enable multipart/form-data encoding
    pub multipart: bool,
}

impl<'a> Form<'a> {
    /// Create a new form with the given action and method.
    #[must_use]
    pub fn new(action: &'a str, method: &'a str, content: Markup) -> Self {
        Self {
            action,
            method,
            content,
            class: None,
            id: None,
            multipart: false,
        }
    }

    /// Create a POST form.
    #[must_use]
    pub fn post(action: &'a str, content: Markup) -> Self {
        Self::new(action, "post", content)
    }

    /// Create a GET form.
    #[must_use]
    pub fn get(action: &'a str, content: Markup) -> Self {
        Self::new(action, "get", content)
    }

    /// Set the CSS class.
    #[must_use]
    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }

    /// Set the form ID.
    #[must_use]
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Enable multipart/form-data encoding (for file uploads).
    #[must_use]
    pub fn multipart(mut self) -> Self {
        self.multipart = true;
        self
    }
}

impl Render for Form<'_> {
    fn render(&self) -> Markup {
        html! {
            form
                action=(self.action)
                method=(self.method)
                class=[self.class]
                id=[self.id]
                enctype=[self.multipart.then_some("multipart/form-data")]
            {
                (self.content)
            }
        }
    }
}

/// An input element.
#[derive(Debug, Clone)]
pub struct Input<'a> {
    /// Input name attribute
    pub name: &'a str,
    /// Input type ("text", "password", "email", "number", "hidden", etc.)
    pub r#type: &'a str,
    /// Current value
    pub value: Option<&'a str>,
    /// Placeholder text
    pub placeholder: Option<&'a str>,
    /// Whether the field is required
    pub required: bool,
    /// Whether the field is disabled
    pub disabled: bool,
    /// Optional ID attribute
    pub id: Option<&'a str>,
    /// Optional CSS class
    pub class: Option<&'a str>,
    /// Autocomplete attribute
    pub autocomplete: Option<&'a str>,
    /// Minimum value (for number inputs)
    pub min: Option<&'a str>,
    /// Maximum value (for number inputs)
    pub max: Option<&'a str>,
    /// Step value (for number inputs)
    pub step: Option<&'a str>,
    /// Pattern for validation
    pub pattern: Option<&'a str>,
    /// Readonly attribute
    pub readonly: bool,
}

impl<'a> Input<'a> {
    /// Create a new input with the given name and type.
    #[must_use]
    pub fn new(name: &'a str, r#type: &'a str) -> Self {
        Self {
            name,
            r#type,
            value: None,
            placeholder: None,
            required: false,
            disabled: false,
            id: None,
            class: None,
            autocomplete: None,
            min: None,
            max: None,
            step: None,
            pattern: None,
            readonly: false,
        }
    }

    /// Create a text input.
    #[must_use]
    pub fn text(name: &'a str) -> Self {
        Self::new(name, "text")
    }

    /// Create a password input.
    #[must_use]
    pub fn password(name: &'a str) -> Self {
        Self::new(name, "password")
    }

    /// Create an email input.
    #[must_use]
    pub fn email(name: &'a str) -> Self {
        Self::new(name, "email")
    }

    /// Create a number input.
    #[must_use]
    pub fn number(name: &'a str) -> Self {
        Self::new(name, "number")
    }

    /// Create a search input.
    #[must_use]
    pub fn search(name: &'a str) -> Self {
        Self::new(name, "search")
    }

    /// Create a URL input.
    #[must_use]
    pub fn url(name: &'a str) -> Self {
        Self::new(name, "url")
    }

    /// Create a hidden input with a value.
    #[must_use]
    pub fn hidden(name: &'a str, value: &'a str) -> Self {
        Self::new(name, "hidden").value(value)
    }

    /// Set the value.
    #[must_use]
    pub fn value(mut self, value: &'a str) -> Self {
        self.value = Some(value);
        self
    }

    /// Set the value if Some.
    #[must_use]
    pub fn value_opt(mut self, value: Option<&'a str>) -> Self {
        self.value = value;
        self
    }

    /// Set the placeholder.
    #[must_use]
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = Some(placeholder);
        self
    }

    /// Mark as required.
    #[must_use]
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Mark as disabled.
    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    /// Set the ID.
    #[must_use]
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the CSS class.
    #[must_use]
    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }

    /// Set the autocomplete attribute.
    #[must_use]
    pub fn autocomplete(mut self, autocomplete: &'a str) -> Self {
        self.autocomplete = Some(autocomplete);
        self
    }

    /// Set the minimum value.
    #[must_use]
    pub fn min(mut self, min: &'a str) -> Self {
        self.min = Some(min);
        self
    }

    /// Set the maximum value.
    #[must_use]
    pub fn max(mut self, max: &'a str) -> Self {
        self.max = Some(max);
        self
    }

    /// Set the step value.
    #[must_use]
    pub fn step(mut self, step: &'a str) -> Self {
        self.step = Some(step);
        self
    }

    /// Set the pattern for validation.
    #[must_use]
    pub fn pattern(mut self, pattern: &'a str) -> Self {
        self.pattern = Some(pattern);
        self
    }

    /// Mark as readonly.
    #[must_use]
    pub fn readonly(mut self) -> Self {
        self.readonly = true;
        self
    }
}

impl Render for Input<'_> {
    fn render(&self) -> Markup {
        html! {
            input
                type=(self.r#type)
                name=(self.name)
                value=[self.value]
                placeholder=[self.placeholder]
                required[self.required]
                disabled[self.disabled]
                readonly[self.readonly]
                id=[self.id]
                class=[self.class]
                autocomplete=[self.autocomplete]
                min=[self.min]
                max=[self.max]
                step=[self.step]
                pattern=[self.pattern];
        }
    }
}

/// A textarea element.
#[derive(Debug)]
pub struct TextArea<'a> {
    /// Textarea name attribute
    pub name: &'a str,
    /// Current value/content
    pub value: Option<&'a str>,
    /// Placeholder text
    pub placeholder: Option<&'a str>,
    /// Number of visible rows
    pub rows: Option<u32>,
    /// Number of visible columns
    pub cols: Option<u32>,
    /// Whether the field is required
    pub required: bool,
    /// Whether the field is disabled
    pub disabled: bool,
    /// Optional ID attribute
    pub id: Option<&'a str>,
    /// Optional CSS class
    pub class: Option<&'a str>,
    /// Readonly attribute
    pub readonly: bool,
}

impl<'a> TextArea<'a> {
    /// Create a new textarea with the given name.
    #[must_use]
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            value: None,
            placeholder: None,
            rows: None,
            cols: None,
            required: false,
            disabled: false,
            id: None,
            class: None,
            readonly: false,
        }
    }

    /// Set the value/content.
    #[must_use]
    pub fn value(mut self, value: &'a str) -> Self {
        self.value = Some(value);
        self
    }

    /// Set the value if Some.
    #[must_use]
    pub fn value_opt(mut self, value: Option<&'a str>) -> Self {
        self.value = value;
        self
    }

    /// Set the placeholder.
    #[must_use]
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = Some(placeholder);
        self
    }

    /// Set the number of rows.
    #[must_use]
    pub fn rows(mut self, rows: u32) -> Self {
        self.rows = Some(rows);
        self
    }

    /// Set the number of columns.
    #[must_use]
    pub fn cols(mut self, cols: u32) -> Self {
        self.cols = Some(cols);
        self
    }

    /// Mark as required.
    #[must_use]
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Mark as disabled.
    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    /// Set the ID.
    #[must_use]
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the CSS class.
    #[must_use]
    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }

    /// Mark as readonly.
    #[must_use]
    pub fn readonly(mut self) -> Self {
        self.readonly = true;
        self
    }
}

impl Render for TextArea<'_> {
    fn render(&self) -> Markup {
        html! {
            textarea
                name=(self.name)
                placeholder=[self.placeholder]
                rows=[self.rows]
                cols=[self.cols]
                required[self.required]
                disabled[self.disabled]
                readonly[self.readonly]
                id=[self.id]
                class=[self.class]
            {
                @if let Some(value) = self.value {
                    (value)
                }
            }
        }
    }
}

/// A select dropdown element.
#[derive(Debug)]
pub struct Select<'a> {
    /// Select name attribute
    pub name: &'a str,
    /// Available options
    pub options: Vec<SelectOption<'a>>,
    /// Currently selected value
    pub selected: Option<&'a str>,
    /// Optional ID attribute
    pub id: Option<&'a str>,
    /// Optional CSS class
    pub class: Option<&'a str>,
    /// Whether the field is required
    pub required: bool,
    /// Whether the field is disabled
    pub disabled: bool,
}

impl<'a> Select<'a> {
    /// Create a new select with the given name.
    #[must_use]
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            options: Vec::new(),
            selected: None,
            id: None,
            class: None,
            required: false,
            disabled: false,
        }
    }

    /// Add options to the select.
    #[must_use]
    pub fn options(mut self, options: Vec<SelectOption<'a>>) -> Self {
        self.options = options;
        self
    }

    /// Add a single option.
    #[must_use]
    pub fn option(mut self, value: &'a str, label: &'a str) -> Self {
        self.options.push(SelectOption { value, label });
        self
    }

    /// Set the selected value.
    #[must_use]
    pub fn selected(mut self, selected: &'a str) -> Self {
        self.selected = Some(selected);
        self
    }

    /// Set the selected value if Some.
    #[must_use]
    pub fn selected_opt(mut self, selected: Option<&'a str>) -> Self {
        self.selected = selected;
        self
    }

    /// Set the ID.
    #[must_use]
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the CSS class.
    #[must_use]
    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }

    /// Mark as required.
    #[must_use]
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Mark as disabled.
    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

impl Render for Select<'_> {
    fn render(&self) -> Markup {
        html! {
            select
                name=(self.name)
                id=[self.id]
                class=[self.class]
                required[self.required]
                disabled[self.disabled]
            {
                @for opt in &self.options {
                    option
                        value=(opt.value)
                        selected[self.selected == Some(opt.value)]
                    {
                        (opt.label)
                    }
                }
            }
        }
    }
}

/// An option for a select element.
#[derive(Debug, Clone)]
pub struct SelectOption<'a> {
    /// Option value
    pub value: &'a str,
    /// Option display label
    pub label: &'a str,
}

impl<'a> SelectOption<'a> {
    /// Create a new select option.
    #[must_use]
    pub fn new(value: &'a str, label: &'a str) -> Self {
        Self { value, label }
    }
}

/// A label element for form inputs.
#[derive(Debug)]
pub struct Label<'a> {
    /// The ID of the input this label is for
    pub r#for: &'a str,
    /// Label text
    pub text: &'a str,
    /// Optional CSS class
    pub class: Option<&'a str>,
}

impl<'a> Label<'a> {
    /// Create a new label.
    #[must_use]
    pub fn new(r#for: &'a str, text: &'a str) -> Self {
        Self {
            r#for,
            text,
            class: None,
        }
    }

    /// Set the CSS class.
    #[must_use]
    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }
}

impl Render for Label<'_> {
    fn render(&self) -> Markup {
        html! {
            label for=(self.r#for) class=[self.class] {
                (self.text)
            }
        }
    }
}

/// A form help/hint text element (uses `<small>` tag).
#[derive(Debug)]
pub struct FormHelp<'a> {
    /// Help text content
    pub text: &'a str,
    /// Optional CSS class
    pub class: Option<&'a str>,
}

impl<'a> FormHelp<'a> {
    /// Create new form help text.
    #[must_use]
    pub fn new(text: &'a str) -> Self {
        Self { text, class: None }
    }

    /// Set the CSS class.
    #[must_use]
    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }
}

impl Render for FormHelp<'_> {
    fn render(&self) -> Markup {
        html! {
            small class=[self.class] {
                (self.text)
            }
        }
    }
}

/// A hidden input element (convenience wrapper).
#[derive(Debug)]
pub struct HiddenInput<'a> {
    /// Input name
    pub name: &'a str,
    /// Input value
    pub value: &'a str,
}

impl<'a> HiddenInput<'a> {
    /// Create a new hidden input.
    #[must_use]
    pub fn new(name: &'a str, value: &'a str) -> Self {
        Self { name, value }
    }
}

impl Render for HiddenInput<'_> {
    fn render(&self) -> Markup {
        html! {
            input type="hidden" name=(self.name) value=(self.value);
        }
    }
}

/// A form group container for label + input + help text.
#[derive(Debug)]
pub struct FormGroup<'a> {
    /// Label text
    pub label: &'a str,
    /// Input ID (also used for label's `for` attribute)
    pub id: &'a str,
    /// The input element
    pub input: Markup,
    /// Optional help text
    pub help: Option<&'a str>,
    /// Optional CSS class for the container
    pub class: Option<&'a str>,
}

impl<'a> FormGroup<'a> {
    /// Create a new form group.
    #[must_use]
    pub fn new(label: &'a str, id: &'a str, input: Markup) -> Self {
        Self {
            label,
            id,
            input,
            help: None,
            class: None,
        }
    }

    /// Add help text.
    #[must_use]
    pub fn help(mut self, help: &'a str) -> Self {
        self.help = Some(help);
        self
    }

    /// Set the container CSS class.
    #[must_use]
    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }
}

impl Render for FormGroup<'_> {
    fn render(&self) -> Markup {
        html! {
            div class=[self.class] {
                label for=(self.id) { (self.label) }
                (self.input)
                @if let Some(help) = self.help {
                    small { (help) }
                }
            }
        }
    }
}

/// A checkbox input element.
#[derive(Debug)]
pub struct Checkbox<'a> {
    /// Input name
    pub name: &'a str,
    /// Input value (defaults to "1" if not set)
    pub value: Option<&'a str>,
    /// Whether the checkbox is checked
    pub checked: bool,
    /// Label text (displayed after the checkbox)
    pub label: Option<&'a str>,
    /// Optional ID attribute
    pub id: Option<&'a str>,
    /// Whether the field is disabled
    pub disabled: bool,
}

impl<'a> Checkbox<'a> {
    /// Create a new checkbox.
    #[must_use]
    pub fn new(name: &'a str) -> Self {
        Self {
            name,
            value: None,
            checked: false,
            label: None,
            id: None,
            disabled: false,
        }
    }

    /// Set the value.
    #[must_use]
    pub fn value(mut self, value: &'a str) -> Self {
        self.value = Some(value);
        self
    }

    /// Mark as checked.
    #[must_use]
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Set the label text.
    #[must_use]
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Set the ID.
    #[must_use]
    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Mark as disabled.
    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

impl Render for Checkbox<'_> {
    fn render(&self) -> Markup {
        let input_html = html! {
            input
                type="checkbox"
                name=(self.name)
                value=(self.value.unwrap_or("1"))
                checked[self.checked]
                disabled[self.disabled]
                id=[self.id];
        };

        if let Some(label_text) = self.label {
            html! {
                label {
                    (input_html)
                    " "
                    (label_text)
                }
            }
        } else {
            input_html
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_form_render() {
        let content = html! { input type="text" name="test"; };
        let form = Form::post("/submit", content);
        let markup = form.render();
        let html = markup.into_string();

        assert!(html.contains(r#"action="/submit""#));
        assert!(html.contains(r#"method="post""#));
        assert!(html.contains(r#"name="test""#));
    }

    #[test]
    fn test_form_with_class_and_id() {
        let content = html! {};
        let form = Form::get("/search", content)
            .class("search-form")
            .id("main-search");
        let html = form.render().into_string();

        assert!(html.contains(r#"class="search-form""#));
        assert!(html.contains(r#"id="main-search""#));
    }

    #[test]
    fn test_form_multipart() {
        let content = html! {};
        let form = Form::post("/upload", content).multipart();
        let html = form.render().into_string();

        assert!(html.contains(r#"enctype="multipart/form-data""#));
    }

    #[test]
    fn test_input_text() {
        let input = Input::text("username")
            .placeholder("Enter username")
            .required();
        let html = input.render().into_string();

        assert!(html.contains(r#"type="text""#));
        assert!(html.contains(r#"name="username""#));
        assert!(html.contains(r#"placeholder="Enter username""#));
        assert!(html.contains("required"));
    }

    #[test]
    fn test_input_password() {
        let input = Input::password("password").autocomplete("current-password");
        let html = input.render().into_string();

        assert!(html.contains(r#"type="password""#));
        assert!(html.contains(r#"autocomplete="current-password""#));
    }

    #[test]
    fn test_input_hidden() {
        let input = Input::hidden("csrf_token", "abc123");
        let html = input.render().into_string();

        assert!(html.contains(r#"type="hidden""#));
        assert!(html.contains(r#"name="csrf_token""#));
        assert!(html.contains(r#"value="abc123""#));
    }

    #[test]
    fn test_input_number_with_min_max() {
        let input = Input::number("quantity")
            .min("1")
            .max("100")
            .step("1")
            .value("10");
        let html = input.render().into_string();

        assert!(html.contains(r#"type="number""#));
        assert!(html.contains(r#"min="1""#));
        assert!(html.contains(r#"max="100""#));
        assert!(html.contains(r#"step="1""#));
        assert!(html.contains(r#"value="10""#));
    }

    #[test]
    fn test_input_disabled_and_readonly() {
        let input = Input::text("readonly_field")
            .value("fixed value")
            .disabled()
            .readonly();
        let html = input.render().into_string();

        assert!(html.contains("disabled"));
        assert!(html.contains("readonly"));
    }

    #[test]
    fn test_textarea_render() {
        let textarea = TextArea::new("content")
            .placeholder("Enter your message")
            .rows(5)
            .value("Hello world");
        let html = textarea.render().into_string();

        assert!(html.contains(r#"name="content""#));
        assert!(html.contains(r#"placeholder="Enter your message""#));
        assert!(html.contains(r#"rows="5""#));
        assert!(html.contains("Hello world"));
    }

    #[test]
    fn test_textarea_empty() {
        let textarea = TextArea::new("notes");
        let html = textarea.render().into_string();

        assert!(html.contains(r#"name="notes""#));
        assert!(html.contains("<textarea"));
        assert!(html.contains("</textarea>"));
    }

    #[test]
    fn test_select_render() {
        let select = Select::new("status")
            .option("pending", "Pending")
            .option("approved", "Approved")
            .option("rejected", "Rejected")
            .selected("approved");
        let html = select.render().into_string();

        assert!(html.contains(r#"name="status""#));
        assert!(html.contains(r#"value="pending""#));
        assert!(html.contains(r#"value="approved" selected"#));
        assert!(html.contains("Approved"));
    }

    #[test]
    fn test_select_with_options_vec() {
        let options = vec![
            SelectOption::new("a", "Option A"),
            SelectOption::new("b", "Option B"),
        ];
        let select = Select::new("choice").options(options);
        let html = select.render().into_string();

        assert!(html.contains(r#"value="a""#));
        assert!(html.contains("Option A"));
        assert!(html.contains(r#"value="b""#));
        assert!(html.contains("Option B"));
    }

    #[test]
    fn test_label_render() {
        let label = Label::new("email", "Email Address");
        let html = label.render().into_string();

        assert!(html.contains(r#"for="email""#));
        assert!(html.contains("Email Address"));
    }

    #[test]
    fn test_form_help_render() {
        let help = FormHelp::new("Enter a valid email address");
        let html = help.render().into_string();

        assert!(html.contains("<small"));
        assert!(html.contains("Enter a valid email address"));
    }

    #[test]
    fn test_hidden_input_render() {
        let hidden = HiddenInput::new("action", "update");
        let html = hidden.render().into_string();

        assert!(html.contains(r#"type="hidden""#));
        assert!(html.contains(r#"name="action""#));
        assert!(html.contains(r#"value="update""#));
    }

    #[test]
    fn test_form_group_render() {
        let input = Input::text("email").id("email").render();
        let group = FormGroup::new("Email", "email", input).help("We'll never share your email");
        let html = group.render().into_string();

        assert!(html.contains(r#"for="email""#));
        assert!(html.contains("Email"));
        assert!(html.contains(r#"id="email""#));
        assert!(html.contains("<small"));
        assert!(html.contains("never share"));
    }

    #[test]
    fn test_checkbox_unchecked() {
        let checkbox = Checkbox::new("remember").label("Remember me");
        let html = checkbox.render().into_string();

        assert!(html.contains(r#"type="checkbox""#));
        assert!(html.contains(r#"name="remember""#));
        assert!(html.contains("Remember me"));
        assert!(!html.contains("checked"));
    }

    #[test]
    fn test_checkbox_checked() {
        let checkbox = Checkbox::new("agree").value("yes").checked(true);
        let html = checkbox.render().into_string();

        assert!(html.contains(r#"value="yes""#));
        assert!(html.contains("checked"));
    }

    #[test]
    fn test_checkbox_default_value() {
        let checkbox = Checkbox::new("active");
        let html = checkbox.render().into_string();

        assert!(html.contains(r#"value="1""#));
    }

    #[test]
    fn test_input_value_opt() {
        let value: Option<&str> = Some("test");
        let input = Input::text("field").value_opt(value);
        let html = input.render().into_string();
        assert!(html.contains(r#"value="test""#));

        let none_value: Option<&str> = None;
        let input2 = Input::text("field").value_opt(none_value);
        let html2 = input2.render().into_string();
        assert!(!html2.contains("value="));
    }

    #[test]
    fn test_select_selected_opt() {
        let selected: Option<&str> = Some("b");
        let select = Select::new("choice")
            .option("a", "A")
            .option("b", "B")
            .selected_opt(selected);
        let html = select.render().into_string();
        assert!(html.contains(r#"value="b" selected"#));
    }
}
