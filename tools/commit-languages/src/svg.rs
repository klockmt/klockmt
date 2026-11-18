use crate::collect::StatsTotals;

const TITLE_Y: f64 = 22.0;
const FIRST_ROW_Y: f64 = 50.0;
const ROW_HEIGHT: f64 = 30.0;
const LANGUAGE_LABEL_WIDTH: f64 = 100.0;
const LANGUAGE_BAR_MAX_WIDTH: f64 = 180.0;
const LANGUAGE_CANVAS_WIDTH: f64 = 450.0;
const STATS_VALUE_X: f64 = 190.0;
const STATS_CANVAS_WIDTH: f64 = 270.0;
const STATS_TITLE: &str = "Geral";
const LANGUAGES_TITLE: &str = "Top commit languages";

pub fn render(top: &[(&str, usize)], total_commits: usize) -> String {
    if top.is_empty() {
        let body = "  <text x=\"0\" y=\"50\" fill=\"#888\">no commits found</text>";
        return wrap(
            LANGUAGE_CANVAS_WIDTH,
            LANGUAGES_TITLE,
            FIRST_ROW_Y + ROW_HEIGHT,
            body,
        );
    }
    let largest = top[0].1 as f64;
    let rows: Vec<String> = top
        .iter()
        .enumerate()
        .map(|(index, (name, count))| language_row(index, name, *count, largest, total_commits))
        .collect();
    let height = FIRST_ROW_Y + ROW_HEIGHT * top.len() as f64;
    wrap(
        LANGUAGE_CANVAS_WIDTH,
        LANGUAGES_TITLE,
        height,
        &rows.join("\n"),
    )
}

pub fn render_stats(totals: &StatsTotals) -> String {
    let rows = [
        ("Commits", totals.commits),
        ("Pull requests", totals.pull_requests),
        ("Issues", totals.issues),
        ("Stars", totals.stars),
        ("Projects contributed to", totals.contributed_repos),
    ];
    let body: Vec<String> = rows
        .iter()
        .enumerate()
        .map(|(index, (label, value))| stat_row(index, label, *value))
        .collect();
    let height = FIRST_ROW_Y + ROW_HEIGHT * rows.len() as f64;
    wrap(STATS_CANVAS_WIDTH, STATS_TITLE, height, &body.join("\n"))
}

fn stat_row(index: usize, label: &str, value: u64) -> String {
    let text_y = row_text_y(index);
    format!(
        "  <text x=\"0\" y=\"{text_y}\" fill=\"#888\">{label}</text>\n  \
         <text x=\"{STATS_VALUE_X}\" y=\"{text_y}\" font-weight=\"600\" fill=\"#2f81f7\">{value}</text>",
        label = escape(label),
    )
}

fn language_row(
    index: usize,
    name: &str,
    count: usize,
    largest: f64,
    total_commits: usize,
) -> String {
    let y = FIRST_ROW_Y + ROW_HEIGHT * index as f64;
    let text_y = row_text_y(index);
    let bar = (LANGUAGE_BAR_MAX_WIDTH * count as f64 / largest)
        .round()
        .max(1.0);
    let percent = 100.0 * count as f64 / total_commits as f64;
    let label_x = LANGUAGE_LABEL_WIDTH + bar + 8.0;
    format!(
        "  <text x=\"0\" y=\"{text_y}\" fill=\"#888\">{name}</text>\n  \
         <rect x=\"{LANGUAGE_LABEL_WIDTH}\" y=\"{y}\" width=\"{bar}\" height=\"14\" rx=\"3\" fill=\"#2f81f7\"/>\n  \
         <text x=\"{label_x}\" y=\"{text_y}\" fill=\"#888\">{count} commits ({percent:.1}%)</text>",
        name = escape(name),
    )
}

fn row_text_y(index: usize) -> f64 {
    FIRST_ROW_Y + ROW_HEIGHT * index as f64 + 12.0
}

fn wrap(width: f64, title: &str, height: f64, body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         font-family=\"Segoe UI, Ubuntu, sans-serif\" font-size=\"14\">\n  \
         <text x=\"0\" y=\"{TITLE_Y}\" font-size=\"16\" font-weight=\"700\" fill=\"#2f81f7\">{title}</text>\n\
         {body}\n</svg>\n",
        title = escape(title),
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

    fn sample_totals() -> StatsTotals {
        StatsTotals {
            commits: 800,
            pull_requests: 12,
            issues: 4,
            stars: 9,
            contributed_repos: 3,
        }
    }

    #[test]
    fn lists_languages_with_share_of_all_commits() {
        let svg = render(&[("Python", 3), ("Shell", 1)], 4);
        assert!(svg.contains("Python"));
        assert!(svg.contains("3 commits (75.0%)"));
    }

    #[test]
    fn titles_each_card_as_requested() {
        assert!(render(&[("Python", 1)], 1).contains(">Top commit languages<"));
        assert!(render_stats(&sample_totals()).contains(">Geral<"));
    }

    #[test]
    fn renders_stats_rows_with_values_including_projects() {
        let svg = render_stats(&sample_totals());
        for expected in [
            "Commits",
            "Pull requests",
            "Issues",
            "Stars",
            "Projects contributed to",
            ">800<",
            ">12<",
            ">9<",
            ">3<",
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
