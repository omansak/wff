use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use fff_search::file_picker::FilePicker;
use fff_search::{FFFMode, FilePickerOptions, FuzzySearchOptions, GrepSearchOptions, PaginationArgs, QueryParser, SharedFilePicker, SharedFrecency, parse_grep_query};

pub const MIN_QUERY_LEN: usize = 3;

const MAX_FILE_HITS: usize = 30;
const MAX_CONTENT_HITS: usize = 150;

/// Time budget for the content scan, which runs on every keystroke.
const SEARCH_BUDGET_MS: u64 = 300;

const SCAN_TIMEOUT: Duration = Duration::from_secs(120);

/// Built on the worker thread; Slint models are not `Send`, so this stays plain data.
pub struct Row {
    pub header: String,
    pub header_spans: Vec<(u32, u32)>,
    pub body: String,
    pub body_spans: Vec<(u32, u32)>,
    pub has_body: bool,
    pub path: String,
    pub line: i32,
}

pub struct Outcome {
    pub rows: Vec<Row>,
    pub name_hits: usize,
    pub content_hits: usize,
}

pub fn start_indexing(picker: &SharedFilePicker, frecency: &SharedFrecency, folder: &str) -> Result<(), fff_search::Error> {
    let options = FilePickerOptions {
        base_path: folder.to_string(),
        mode: FFFMode::Neovim,
        watch: false,
        ..Default::default()
    };

    FilePicker::new_with_shared_state(picker.clone(), frecency.clone(), options)
}

pub fn wait_for_scan(picker: &SharedFilePicker) -> usize {
    picker.wait_for_scan(SCAN_TIMEOUT);
    picker.read().ok().and_then(|guard| guard.as_ref().map(|picker| picker.get_files().len())).unwrap_or(0)
}

/// Rows come back ordered: definitions, literal name matches, other content lines, fuzzy name matches.
pub fn run(picker: &FilePicker, query: &str, cancelled: &Arc<AtomicBool>) -> Outcome {
    let needle = query.to_lowercase();

    let mut definitions = Vec::new();
    let mut name_exact = Vec::new();
    let mut other_lines = Vec::new();
    let mut name_loose = Vec::new();

    let parser = QueryParser::default();
    let parsed = parser.parse(query);

    let by_name = picker.fuzzy_search(
        &parsed,
        None,
        FuzzySearchOptions {
            pagination: PaginationArgs { offset: 0, limit: MAX_FILE_HITS },
            ..Default::default()
        },
    );

    let base = picker.base_path();

    for (item, spans) in by_name.items.iter().zip(&by_name.match_byte_offsets) {
        let path = item.relative_path(picker);
        let matches_literally = path.to_lowercase().contains(&needle);

        let row = Row {
            path: base.join(&path).display().to_string(),
            header: path,
            header_spans: spans.to_vec(),
            body: String::new(),
            body_spans: Vec::new(),
            has_body: false,
            line: 0,
        };

        if matches_literally {
            name_exact.push(row);
        } else {
            name_loose.push(row);
        }
    }

    let grep_query = parse_grep_query(query);
    let by_content = picker.grep(
        &grep_query,
        &GrepSearchOptions {
            page_limit: MAX_CONTENT_HITS,
            trim_whitespace: true,
            classify_definitions: true,
            time_budget_ms: SEARCH_BUDGET_MS,
            enforce_time_budget: true,
            abort_signal: Some(cancelled.clone()),
            ..Default::default()
        },
    );

    for hit in &by_content.matches {
        let path = by_content.files[hit.file_index].relative_path(picker);

        let row = Row {
            header: format!("{path}:{}", hit.line_number),
            header_spans: Vec::new(),
            body: hit.line_content.clone(),
            body_spans: hit.match_byte_offsets.to_vec(),
            has_body: true,
            path: base.join(&path).display().to_string(),
            line: hit.line_number as i32,
        };

        if hit.is_definition {
            definitions.push(row);
        } else {
            other_lines.push(row);
        }
    }

    let name_hits = name_exact.len() + name_loose.len();
    let content_hits = definitions.len() + other_lines.len();

    let mut rows = definitions;
    rows.extend(name_exact);
    rows.extend(other_lines);
    rows.extend(name_loose);

    Outcome { rows, name_hits, content_hits }
}
