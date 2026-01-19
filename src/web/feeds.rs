use crate::db::Archive;

/// Generate RSS 2.0 feed XML
pub fn generate_rss(archives: &[Archive], base_url: &str) -> String {
    let items: String = archives
        .iter()
        .map(|archive| {
            let title = xml_escape(
                archive
                    .content_title
                    .as_deref()
                    .unwrap_or("Untitled Archive"),
            );
            let link = format!("{}/archive/{}", base_url, archive.id);
            let description = xml_escape(archive.content_text.as_deref().unwrap_or(""));
            let pub_date = archive.archived_at.as_deref().unwrap_or("");
            let content_type = archive.content_type.as_deref().unwrap_or("unknown");

            format!(
                r#"    <item>
      <title>{title}</title>
      <link>{link}</link>
      <guid isPermaLink="true">{link}</guid>
      <description><![CDATA[{description}]]></description>
      <pubDate>{pub_date}</pubDate>
      <category>{content_type}</category>
    </item>"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
  <channel>
    <title>Discourse Link Archiver - New Archives</title>
    <link>{base_url}</link>
    <description>Recently archived content from the Discourse Link Archiver</description>
    <language>en-us</language>
    <atom:link href="{base_url}/feed.rss" rel="self" type="application/rss+xml"/>
{items}
  </channel>
</rss>"#
    )
}

/// Generate Atom 1.0 feed XML
pub fn generate_atom(archives: &[Archive], base_url: &str) -> String {
    let now = chrono::Utc::now().to_rfc3339();

    let entries: String = archives
        .iter()
        .map(|archive| {
            let title = xml_escape(
                archive
                    .content_title
                    .as_deref()
                    .unwrap_or("Untitled Archive"),
            );
            let link = format!("{}/archive/{}", base_url, archive.id);
            let summary = xml_escape(archive.content_text.as_deref().unwrap_or(""));
            let updated = archive.archived_at.as_deref().unwrap_or(&now);
            let author = xml_escape(archive.content_author.as_deref().unwrap_or("Unknown"));
            let content_type = archive.content_type.as_deref().unwrap_or("unknown");

            format!(
                r#"  <entry>
    <title>{title}</title>
    <link href="{link}" rel="alternate" type="text/html"/>
    <id>{link}</id>
    <updated>{updated}</updated>
    <author><name>{author}</name></author>
    <summary><![CDATA[{summary}]]></summary>
    <category term="{content_type}"/>
  </entry>"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Discourse Link Archiver - New Archives</title>
  <link href="{base_url}" rel="alternate" type="text/html"/>
  <link href="{base_url}/feed.atom" rel="self" type="application/atom+xml"/>
  <id>{base_url}/</id>
  <updated>{now}</updated>
  <subtitle>Recently archived content from the Discourse Link Archiver</subtitle>
{entries}
</feed>"#
    )
}

/// Escape XML special characters
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_rss_empty() {
        let rss = generate_rss(&[], "https://example.com");
        assert!(rss.contains("<?xml version="));
        assert!(rss.contains("<rss version=\"2.0\""));
        assert!(rss.contains("Discourse Link Archiver"));
    }

    #[test]
    fn test_generate_atom_empty() {
        let atom = generate_atom(&[], "https://example.com");
        assert!(atom.contains("<?xml version="));
        assert!(atom.contains("<feed xmlns="));
        assert!(atom.contains("Discourse Link Archiver"));
    }

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("<script>"), "&lt;script&gt;");
        assert_eq!(xml_escape("a & b"), "a &amp; b");
        assert_eq!(xml_escape("\"test\""), "&quot;test&quot;");
    }
}
