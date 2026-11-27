use crate::models::{Program, ProgramField};
use scraper::{ElementRef, Html, Selector};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPageData {
    pub program: Program,
    pub fields: Vec<ProgramField>,
}

/// Parses HTML content from YCM page (YCM321-3W2) and extracts program info and fields.
pub fn parse_ycm_html(html_str: &str) -> anyhow::Result<ParsedPageData> {
    let document = Html::parse_document(html_str);

    // 1. Basic Info
    let job_code_sel = Selector::parse("#jobCodeNumber").map_err(|e| anyhow::anyhow!("{:?}", e))?;
    let job_name_sel = Selector::parse("#jobName").map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let job_code = document
        .select(&job_code_sel)
        .next()
        .and_then(|el| el.value().attr("value"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let job_name = document
        .select(&job_name_sel)
        .next()
        .and_then(|el| el.value().attr("value"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let mut fields = Vec::new();
    let mut sort_order = 0;

    // 2. Job Parameters (#jobParaTb tr.row)
    let job_para_row_sel =
        Selector::parse("table#jobParaTb tr.row").map_err(|e| anyhow::anyhow!("{:?}", e))?;

    for row in document.select(&job_para_row_sel) {
        if let Some(field) = extract_field_from_row(&row, "JOB", sort_order) {
            fields.push(field);
            sort_order += 1;
        }
    }

    // 3. Program Parameters (#programSet)
    let prog_param_row_sel = Selector::parse("#programSet tr[id^='tr_programParameter_']")
        .map_err(|e| anyhow::anyhow!("{:?}", e))?;
    let prog_param_fallback_sel =
        Selector::parse("#programSet tr.row").map_err(|e| anyhow::anyhow!("{:?}", e))?;

    let mut prog_rows: Vec<ElementRef> = document.select(&prog_param_row_sel).collect();
    if prog_rows.is_empty() {
        prog_rows = document.select(&prog_param_fallback_sel).collect();
    }

    let mut seen_ids = std::collections::HashSet::new();

    for row in prog_rows {
        if let Some(field) = extract_field_from_row(&row, "PROGRAM", sort_order) {
            if seen_ids.insert(field.html_id.clone()) {
                fields.push(field);
                sort_order += 1;
            }
        }
    }

    Ok(ParsedPageData {
        program: Program {
            id: None,
            job_code,
            job_name,
            created_at: None,
        },
        fields,
    })
}

fn extract_field_from_row(
    row: &ElementRef,
    scope: &str,
    sort_order: i32,
) -> Option<ProgramField> {
    let th_sel = Selector::parse("th").ok()?;
    let th_el = row.select(&th_sel).next();

    let (label, th_required) = if let Some(th) = th_el {
        let raw_text = th.text().collect::<Vec<_>>().join(" ");
        let has_asterisk = raw_text.contains('*')
            || th
                .select(&Selector::parse("font[color='red']").unwrap())
                .any(|f| f.text().any(|t| t.contains('*')));
        let cleaned_label = raw_text
            .replace('*', "")
            .replace('：', "")
            .replace(':', "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        (cleaned_label, has_asterisk)
    } else {
        (String::new(), false)
    };

    let td_sel = Selector::parse("td").ok()?;
    let td_el = row.select(&td_sel).next()?;

    let input_sel = Selector::parse("input, select, textarea").ok()?;
    for el in td_el.select(&input_sel) {
        let tag_name = el.value().name().to_string();
        let input_type = el.value().attr("type").unwrap_or("text").to_lowercase();

        if tag_name == "input" && input_type == "hidden" {
            continue;
        }

        let html_id = el
            .value()
            .attr("id")
            .or_else(|| el.value().attr("name"))
            .unwrap_or_default()
            .trim()
            .to_string();

        if html_id.is_empty() {
            continue;
        }

        let html_name = el
            .value()
            .attr("name")
            .unwrap_or(&html_id)
            .trim()
            .to_string();

        let max_length = el
            .value()
            .attr("maxlength")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);

        let validate_attr = el.value().attr("validate").unwrap_or_default();
        let class_attr = el.value().attr("class").unwrap_or_default();
        let has_required_attr = el.value().attr("required").is_some();

        let is_required = th_required
            || validate_attr.contains("required:true")
            || validate_attr.contains("required : true")
            || class_attr.contains("required")
            || has_required_attr;

        return Some(ProgramField {
            id: None,
            program_id: 0,
            field_scope: scope.to_string(),
            html_id,
            html_name,
            label,
            tag_type: tag_name,
            max_length,
            is_required,
            sort_order,
            year_ad: false,
        });
    }

    None
}
