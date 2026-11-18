const LABEL_WIDTH: f64 = 110.0;
const BAR_MAX_WIDTH: f64 = 260.0;
const ROW_HEIGHT: f64 = 30.0;
const FIRST_ROW_Y: f64 = 20.0;
const CANVAS_WIDTH: f64 = 540.0;
const VALUE_X: f64 = 160.0;

pub fn render(top: &[(&str, usize)], total_commits: usize) -> String {
    if top.is_empty() {
        return wrap(
            FIRST_ROW_Y + ROW_HEIGHT,
            "  <text x=\"10\" y=\"30\">no commits found</text>",
        );
    }
    let largest = top[0].1 as f64;
    let rows: Vec<String> = top
        .iter()
        .enumerate()
        .map(|(index, (name, count))| row(index, name, *count, largest, total_commits))
        .collect();
    let height = FIRST_ROW_Y + ROW_HEIGHT * top.len() as f64;
    wrap(height, &rows.join("\n"))
}

pub fn render_stats(totals: &crate::collect::StatsTotals) -> String {
    let rows = [
        ("Commits", totals.commits),
        ("Pull requests", totals.pull_requests),
        ("Issues", totals.issues),
        ("Stars", totals.stars),
    ];
    let body: Vec<String> = rows
        .iter()
        .enumerate()
        .map(|(index, (label, value))| stat_row(index, label, *value))
        .collect();
    wrap(
        FIRST_ROW_Y + ROW_HEIGHT * rows.len() as f64,
        &body.join("\n"),
    )
}

fn stat_row(index: usize, label: &str, value: u64) -> String {
    let y = FIRST_ROW_Y + ROW_HEIGHT * index as f64;
    format!(
        "  <text x=\"0\" y=\"{text_y}\" fill=\"#888\">{label}</text>\n  \
         <text x=\"{VALUE_X}\" y=\"{text_y}\" font-weight=\"600\" fill=\"#2f81f7\">{value}</text>",
        text_y = y + 12.0,
        label = escape(label),
    )
}

fn wrap(height: f64, body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{CANVAS_WIDTH}\" height=\"{height}\" \
         font-family=\"Segoe UI, Ubuntu, sans-serif\" font-size=\"14\">\n{body}\n</svg>\n"
    )
}

fn row(index: usize, name: &str, count: usize, largest: f64, total_commits: usize) -> String {
    let y = FIRST_ROW_Y + ROW_HEIGHT * index as f64;
    let bar = (BAR_MAX_WIDTH * count as f64 / largest).round().max(1.0);
    let percent = 100.0 * count as f64 / total_commits as f64;
    let label_x = LABEL_WIDTH + bar + 8.0;
    format!(
        "  <text x=\"0\" y=\"{text_y}\" fill=\"#888\">{name}</text>\n  \
         <rect x=\"{LABEL_WIDTH}\" y=\"{y}\" width=\"{bar}\" height=\"14\" rx=\"3\" fill=\"#2f81f7\"/>\n  \
         <text x=\"{label_x}\" y=\"{text_y}\" fill=\"#888\">{count} commits ({percent:.1}%)</text>",
        text_y = y + 12.0,
        name = escape(name),
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_languages_with_share_of_all_commits() {
        let svg = render(&[("Python", 3), ("Shell", 1)], 4);
        assert!(svg.contains("Python"));
        assert!(svg.contains("3 commits (75.0%)"));
    }

    #[test]
    fn renders_stats_rows_with_values() {
        let totals = crate::collect::StatsTotals {
            commits: 800,
            pull_requests: 12,
            issues: 4,
            stars: 9,
        };
        let svg = render_stats(&totals);
        for expected in [
            "Commits",
            "Pull requests",
            "Issues",
            "Stars",
            ">800<",
            ">12<",
            ">9<",
        ] {
            assert!(svg.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn escapes_language_names() {
        let svg = render(&[("A<&>", 1)], 1);
        assert!(svg.contains("A&lt;&amp;&gt;"));
    }

    #[test]
    fn renders_placeholder_when_no_language_is_found() {
        assert!(render(&[], 0).contains("no commits found"));
    }
}
