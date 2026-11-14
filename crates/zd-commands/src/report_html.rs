//! The workflow report as one HTML page (decision 0068): styles and a small script inline, nothing fetched, so it opens
//! offline in any browser and can be emailed as it is. Without the script every case's tables show one after another;
//! with it, a case is picked by a tab, a table sorts by a click on its heading, the blocks filter by words typed, and a
//! table saves as CSV.

use std::fmt::Write;

use crate::report::{BlockRow, CaseFigures, GroupRow, Report, civil};

const STYLE: &str = r#"
:root{--bg:#fff;--fg:#1d2228;--mut:#66707a;--line:#dde1e5;--head:#f3f5f7;--ok:#1f7a45;--okbg:#f2faf5;--warn:#b45f06;--bad:#b3261e;--badbg:#fdf2f1;--edit:#1f5fa8}
@media (prefers-color-scheme:dark){:root{--bg:#16191d;--fg:#e4e7ea;--mut:#98a2ad;--line:#2c3238;--head:#1f2429;--ok:#5fc48a;--okbg:#14251b;--warn:#f0a35a;--bad:#ff8a80;--badbg:#2b1715;--edit:#7fb0ef}}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:13px/1.45 "Segoe UI",system-ui,sans-serif}
.wrap{max-width:960px;margin:0 auto;padding:20px 16px 40px}
h1{font-size:20px;margin:2px 0}h2{font-size:15px;margin:22px 0 6px;padding-top:10px;border-top:1px solid var(--line)}
.mut{color:var(--mut)}.tag{display:inline-block;border:1px solid var(--line);border-radius:10px;padding:0 7px;font-size:11px;color:var(--mut);margin:2px 4px 2px 0}
.tag.ok{color:var(--ok)}.tag.warn{color:var(--warn)}.tag.bad{color:var(--bad)}.tag.edit{color:var(--edit)}
.scroll{overflow-x:auto}table{border-collapse:collapse;width:100%;margin:4px 0;font-variant-numeric:tabular-nums}
th,td{padding:3px 7px;border-bottom:1px solid var(--line);text-align:right;white-space:nowrap}
th{background:var(--head);font-weight:600;font-size:12px}th.sort{cursor:pointer}th:first-child,td:first-child,.l{text-align:left}
tr.tot td{font-weight:600;border-top:1.5px solid var(--fg)}.d{color:var(--edit);font-size:11px}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(170px,1fr));gap:8px;margin:10px 0}
.card{border:1px solid var(--line);border-radius:8px;padding:7px 9px}.card b{display:block;font-size:17px}.card span{color:var(--mut);font-size:11px}
.rec{border:1px solid var(--ok);background:var(--okbg);border-radius:8px;padding:7px 10px;margin:6px 0}.rec.bad{border-color:var(--bad);background:var(--badbg)}
.tabs button{font:inherit;border:1px solid var(--line);background:var(--bg);color:var(--fg);border-radius:6px;padding:2px 10px;margin-right:4px;cursor:pointer}
.tabs button.on{border-color:var(--edit);color:var(--edit)}
input.filter{border:1px solid var(--line);border-radius:6px;padding:3px 7px;font:inherit;width:240px;background:var(--bg);color:var(--fg)}
button.csv{font:inherit;font-size:12px;border:none;background:none;color:var(--edit);cursor:pointer}
.step{display:grid;grid-template-columns:28px 1fr;gap:2px 8px;padding:6px 0;border-bottom:1px solid var(--line)}
.step .n{color:var(--mut);text-align:right}.step code{font-size:12px}.note{font-style:italic;color:var(--mut)}.off{opacity:.55}
.toc a{color:var(--edit);text-decoration:none;margin-right:10px}
@media print{.tabs,.filter,button.csv{display:none}[data-case]{display:block!important}}
"#;

const SCRIPT: &str = r#"
(function(){
var cases=[].slice.call(document.querySelectorAll('[data-case]'));
var tabs=[].slice.call(document.querySelectorAll('.tabs button'));
function show(name){cases.forEach(function(c){c.style.display=c.getAttribute('data-case')===name?'':'none'});tabs.forEach(function(b){b.className=b.getAttribute('data-for')===name?'on':''})}
tabs.forEach(function(b){b.onclick=function(){show(b.getAttribute('data-for'))}});
if(tabs.length)show(tabs[0].getAttribute('data-for'));
[].slice.call(document.querySelectorAll('table.sortable')).forEach(function(t){
 [].slice.call(t.tHead.rows[0].cells).forEach(function(th,i){th.className+=' sort';th.title='Sort';th.onclick=function(){
  var body=t.tBodies[0],rows=[].slice.call(body.rows),up=th.getAttribute('data-up')!=='1';th.setAttribute('data-up',up?'1':'0');
  rows.sort(function(a,b){var x=a.cells[i].getAttribute('data-v'),y=b.cells[i].getAttribute('data-v');
   if(x!==null&&y!==null){x=+x;y=+y;return up?x-y:y-x}x=a.cells[i].textContent;y=b.cells[i].textContent;return up?x.localeCompare(y,undefined,{numeric:true}):y.localeCompare(x,undefined,{numeric:true})});
  rows.forEach(function(r){body.appendChild(r)})}})});
[].slice.call(document.querySelectorAll('input.filter')).forEach(function(f){var t=document.getElementById(f.getAttribute('data-table')),n=document.getElementById(f.getAttribute('data-count'));
 f.oninput=function(){var words=f.value.toLowerCase().split(/\s+/).filter(Boolean),k=0;[].slice.call(t.tBodies[0].rows).forEach(function(r){var s=r.textContent.toLowerCase(),ok=words.every(function(w){return s.indexOf(w)>=0});r.style.display=ok?'':'none';if(ok)k++});n.textContent=k}});
[].slice.call(document.querySelectorAll('button.csv')).forEach(function(b){b.onclick=function(){var t=document.getElementById(b.getAttribute('data-table'));
 var lines=[].slice.call(t.rows).filter(function(r){return r.style.display!=='none'}).map(function(r){return [].slice.call(r.cells).map(function(c){var v=c.getAttribute('data-v');v=v===null?c.textContent:v;return /[",\n]/.test(v)?'"'+v.replace(/"/g,'""')+'"':v}).join(',')});
 var a=document.createElement('a');a.href=URL.createObjectURL(new Blob([lines.join('\r\n')],{type:'text/csv'}));a.download=b.getAttribute('data-name')+'.csv';a.click()}});
})();
"#;

/// The page.
pub fn page(r: &Report) -> String {
    let mut h = String::with_capacity(64 * 1024 + r.cases.iter().map(|c| c.blocks.len() * 400).sum::<usize>());
    let (date, time) = civil(r.made_unix_ms, r.utc_offset_minutes);
    let _ = write!(h, "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{} · report</title><style>{STYLE}</style></head><body><div class=\"wrap\">", esc(&r.workflow));
    let _ = write!(h, "<div class=\"mut\">Zovlyn Pit Design · workflow report</div><h1>{}</h1>", esc(&r.workflow));
    let ran: Vec<String> = r.cases.iter().map(|c| match &c.ran {
        Some(ran) => format!("{} ran {} in {:.1} s", c.name, when(ran.at_unix_ms, r.utc_offset_minutes), ran.ms / 1000.0),
        None => format!("{} not run", c.name),
    }).collect();
    let _ = write!(h, "<div class=\"mut\">{}{} · made {date} {time} · {} steps</div>", r.project.as_deref().map(|p| format!("{} · ", esc(p))).unwrap_or_default(), esc(&ran.join(" · ")), r.steps.len());
    let reconciles = r.cases.iter().filter_map(|c| c.pit.as_ref()).all(|p| p.within);
    let has_pit = r.cases.iter().any(|c| c.pit.is_some());
    let warnings: usize = r.cases.iter().map(|c| c.warnings.len()).sum();
    h.push_str("<div>");
    if has_pit {
        h.push_str(if reconciles { "<span class=\"tag ok\">Reconciles to the pit</span>" } else { "<span class=\"tag bad\">Doesn't reconcile to the pit</span>" });
    }
    if warnings > 0 {
        let _ = write!(h, "<span class=\"tag warn\">{warnings} warning{}</span>", s(warnings));
    }
    h.push_str("<span class=\"tag\">Tonnes on the density grid's own basis (usually air dried); in situ comes with the reserves dataflow</span></div>");
    h.push_str("<div class=\"toc\"><a href=\"#summary\">Summary</a><a href=\"#seams\">By seam</a><a href=\"#blocks\">Blocks</a><a href=\"#left-out\">Left out</a><a href=\"#steps\">Steps</a><a href=\"#inputs\">Inputs</a><a href=\"#warnings\">Warnings</a></div>");

    summary(&mut h, r);
    if r.cases.len() > 1 {
        h.push_str("<div class=\"tabs\" style=\"margin-top:14px\">");
        for c in &r.cases {
            let _ = write!(h, "<button data-for=\"{0}\">{0}</button>", esc(&c.name));
        }
        h.push_str("</div>");
    }
    for (k, c) in r.cases.iter().enumerate() {
        let _ = write!(h, "<div data-case=\"{}\">", esc(&c.name));
        by_seam(&mut h, c, k);
        blocks(&mut h, c, k, &r.workflow);
        left_out(&mut h, c);
        case_warnings(&mut h, c);
        h.push_str("</div>");
    }
    steps(&mut h, r);
    inputs(&mut h, r);
    let _ = write!(h, "<p class=\"mut\" style=\"margin-top:24px\">Written by Zovlyn Pit Design from the workflow's runs and the blocks' own attributes (decision 0068). Volumes are each solid's own, the volume of record.</p></div><script>{SCRIPT}</script></body></html>");
    h
}

fn summary(h: &mut String, r: &Report) {
    h.push_str("<h2 id=\"summary\">Summary by case</h2>");
    if let Some(base) = r.cases.first() {
        h.push_str("<div class=\"cards\">");
        card(h, &format!("Coal, {}", base.name), &tonnes_words(base.coal.tonnes), &format!("{} m³{}", int(base.coal.volume), base.coal.rd.map(|rd| format!(" · RD {rd:.3}")).unwrap_or_default()));
        card(h, &format!("Waste, {}", base.name), &format!("{} bcm", int(base.waste.volume)), "overburden and interburden");
        card(h, &format!("Strip ratio, {}", base.name), &base.strip_ratio.map(|sr| format!("{sr:.2}")).unwrap_or("–".into()), "bcm : t");
        card(h, "Blocks", &r.cases.iter().map(|c| int(c.blocks.len() as f64)).collect::<Vec<_>>().join(" / "), &r.cases.iter().map(|c| esc(&c.name)).collect::<Vec<_>>().join(" / "));
        h.push_str("</div>");
    }
    let quals: Vec<&String> = r.cases.first().map(|c| c.qualities.iter().take(2).collect()).unwrap_or_default();
    h.push_str("<div class=\"scroll\"><table><thead><tr><th>Case</th><th>Coal t</th><th>Coal m³</th><th>RD</th>");
    for q in &quals {
        let _ = write!(h, "<th>{}</th>", esc(q));
    }
    h.push_str("<th>Waste bcm</th><th>SR</th><th>Blocks</th></tr></thead><tbody>");
    let base = r.cases.first();
    for (k, c) in r.cases.iter().enumerate() {
        let vs = |f: &dyn Fn(&CaseFigures) -> Option<f64>, dp: usize| -> String {
            let v = f(c);
            let text = v.map(|v| fixed(v, dp)).unwrap_or("–".into());
            match (k > 0, v, base.and_then(|b| f(b))) {
                (true, Some(v), Some(b)) => format!("{text} <span class=\"d\">{}</span>", signed(v - b, dp)),
                _ => text,
            }
        };
        let _ = write!(h, "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td>", esc(&c.name), vs(&|c| c.coal.tonnes, 0), vs(&|c| Some(c.coal.volume), 0), vs(&|c| c.coal.rd, 3));
        for q in &quals {
            let _ = write!(h, "<td>{}</td>", vs(&|c| c.coal.qualities.get(*q).copied(), 2));
        }
        let _ = write!(h, "<td>{}</td><td>{}</td><td>{}</td></tr>", vs(&|c| Some(c.waste.volume), 0), vs(&|c| c.strip_ratio, 2), vs(&|c| Some(c.blocks.len() as f64), 0));
    }
    h.push_str("</tbody></table></div>");
    for c in r.cases.iter().skip(1) {
        if c.changes.is_empty() {
            let _ = write!(h, "<div class=\"mut\">{} changes no setting from Base.</div>", esc(&c.name));
        } else {
            let list: Vec<String> = c.changes.iter().map(|ch| format!("step {} {} {} → {}", ch.step, esc(&ch.field), esc(&json_words(&ch.base)), esc(&json_words(&ch.value)))).collect();
            let _ = write!(h, "<div class=\"mut\">{} changes <span class=\"d\">{}</span>; differences from Base in blue.</div>", esc(&c.name), list.join("; "));
        }
    }
}

fn card(h: &mut String, label: &str, value: &str, sub: &str) {
    let _ = write!(h, "<div class=\"card\"><span>{}</span><b>{}</b><span>{}</span></div>", esc(label), esc(value), sub);
}

fn by_seam(h: &mut String, c: &CaseFigures, k: usize) {
    let _ = write!(h, "<h2 id=\"{}\">By seam and material <span class=\"mut\" style=\"font-weight:400\">({})</span></h2>", if k == 0 { "seams" } else { "" }, esc(&c.name));
    if c.blocks.is_empty() {
        h.push_str("<div class=\"mut\">No blocks: no step of this case made a solid with a block attribute, or it hasn't run.</div>");
        return;
    }
    let _ = write!(h, "<div class=\"scroll\"><table class=\"sortable\" id=\"seams-{k}\"><thead><tr><th>Horizon</th><th class=\"l\">Material</th><th>Volume m³</th><th>Tonnes</th><th>RD</th>");
    for q in &c.qualities {
        let _ = write!(h, "<th>{}</th>", esc(q));
    }
    h.push_str("<th>Blocks</th></tr></thead><tbody>");
    for row in &c.horizons {
        group_row(h, row, &c.qualities, false);
    }
    h.push_str("</tbody><tbody>");
    for row in [&c.coal, &c.waste, &c.below] {
        if row.blocks > 0 {
            group_row(h, row, &c.qualities, true);
        }
    }
    h.push_str("</tbody></table></div>");
    let density = c.density.as_deref().map(|d| format!("RD is '{}', composited by volume; ", esc(d))).unwrap_or_else(|| "No step interrogated the blocks, so there are no tonnes or qualities; ".into());
    let _ = write!(h, "<div class=\"mut\">{density}every other value by coal tonnes (volume × RD), as the interrogation averages them. Waste in bcm; the strip ratio counts waste above the last floor.{}</div>", if c.no_density > 0 { format!(" <span style=\"color:var(--warn)\">{} coal block{} with no density, so no tonnes.</span>", c.no_density, s(c.no_density)) } else { String::new() });
}

fn group_row(h: &mut String, row: &GroupRow, qualities: &[String], total: bool) {
    let _ = write!(h, "<tr{}><td>{}</td><td class=\"l\">{}</td>{}{}{}", if total { " class=\"tot\"" } else { "" }, esc(&row.label), esc(&row.material), num(Some(row.volume), 0), num(row.tonnes, 0), num(row.rd, 3));
    for q in qualities {
        h.push_str(&num(row.qualities.get(q).copied(), 2));
    }
    let _ = write!(h, "{}</tr>", num(Some(row.blocks as f64), 0));
}

fn blocks(h: &mut String, c: &CaseFigures, k: usize, workflow: &str) {
    if c.blocks.is_empty() {
        return;
    }
    let _ = write!(h, "<h2 id=\"{}\">Blocks <span class=\"mut\" style=\"font-weight:400\">({})</span></h2>", if k == 0 { "blocks" } else { "" }, esc(&c.name));
    let _ = write!(
        h,
        "<input class=\"filter\" placeholder=\"Filter: 3 M coal…\" data-table=\"blocks-{k}\" data-count=\"count-{k}\"> <span class=\"mut\"><span id=\"count-{k}\">{n}</span> of {n} · click a heading to sort</span> <button class=\"csv\" data-table=\"blocks-{k}\" data-name=\"{} {} blocks\">Save as CSV</button>",
        esc(workflow),
        esc(&c.name),
        n = c.blocks.len()
    );
    let _ = write!(h, "<div class=\"scroll\"><table class=\"sortable\" id=\"blocks-{k}\"><thead><tr><th>Block</th><th>Strip</th><th class=\"l\">Horizon</th><th class=\"l\">Material</th><th>Volume m³</th><th>Tonnes</th><th>RD</th>");
    for q in &c.qualities {
        let _ = write!(h, "<th>{}</th>", esc(q));
    }
    h.push_str("<th>Column m³</th><th>Δ</th></tr></thead><tbody>");
    for b in &c.blocks {
        block_line(h, b, &c.qualities);
    }
    h.push_str("</tbody></table></div>");
}

fn block_line(h: &mut String, b: &BlockRow, qualities: &[String]) {
    let strip = b.strip.as_deref().unwrap_or("");
    let strip_v = strip.parse::<f64>().map(|v| format!(" data-v=\"{v}\"")).unwrap_or_default();
    let _ = write!(h, "<tr><td>{}</td><td{strip_v}>{}</td><td class=\"l\">{}</td><td class=\"l\">{}</td>{}{}{}", esc(&b.name), esc(strip), esc(b.horizon.as_deref().unwrap_or("")), esc(b.material.as_deref().unwrap_or("")), num(Some(b.volume), 0), num(b.tonnes, 0), num(b.rd, 3));
    for q in qualities {
        h.push_str(&num(b.values.get(q).copied(), 2));
    }
    let delta = b.column.filter(|c| *c != 0.0).map(|c| (b.volume - c) / c * 100.0);
    let _ = write!(h, "{}{}</tr>", num(b.column, 0), delta.map(|d| format!("<td data-v=\"{d}\">{d:.2}%</td>")).unwrap_or("<td></td>".into()));
}

fn left_out(h: &mut String, c: &CaseFigures) {
    let _ = write!(h, "<h2 id=\"left-out\">Left out, and the reconciliation <span class=\"mut\" style=\"font-weight:400\">({})</span></h2>", esc(&c.name));
    match &c.pit {
        Some(p) => {
            let _ = write!(
                h,
                "<div class=\"rec{}\"><b>{} {} m³</b> = blocks {} + left out {} m³ · difference {} m³ ({})</div>",
                if p.within { "" } else { " bad" },
                esc(&p.names.join(", ")),
                fixed(p.volume, 3),
                fixed(c.blocks.iter().map(|b| b.volume).sum(), 3),
                fixed(c.left_out.iter().map(|l| l.volume).sum(), 3),
                fixed(p.difference, 3),
                if p.within { "within 1e-6 of the pit".to_string() } else { format!("{:.2e} of the pit: look at this", p.difference / p.volume) }
            );
        }
        None => h.push_str("<div class=\"mut\">No pit to reconcile to: no step made the solid the cuts start from, or there are no blocks.</div>"),
    }
    if c.left_out.is_empty() {
        h.push_str("<div class=\"mut\">Nothing was left out.</div>");
        return;
    }
    h.push_str("<div class=\"scroll\"><table><thead><tr><th>What</th><th class=\"l\">Step</th><th>Volume m³</th></tr></thead><tbody>");
    for l in &c.left_out {
        let _ = write!(h, "<tr><td>{}</td><td class=\"l\">{}</td>{}</tr>", esc(&l.what), l.step.map(|s| format!("step {s}")).unwrap_or("by hand".into()), num(Some(l.volume), 3));
    }
    h.push_str("</tbody></table></div>");
}

fn case_warnings(h: &mut String, c: &CaseFigures) {
    let _ = write!(h, "<h2 id=\"warnings\">Checks and warnings <span class=\"mut\" style=\"font-weight:400\">({})</span></h2>", esc(&c.name));
    if c.warnings.is_empty() {
        h.push_str("<div class=\"mut\">No step warned.</div>");
        return;
    }
    for w in &c.warnings {
        let tone = if w.severity == "warning" { "warn" } else { "bad" };
        let _ = write!(h, "<div><span class=\"tag {tone}\">{}</span> {} · {}{} · {}</div>", w.severity, w.step.map(|s| format!("step {s}")).unwrap_or("by hand".into()), esc(&w.command), w.code.as_deref().map(|c| format!(" · {}", esc(c))).unwrap_or_default(), esc(&w.text));
    }
}

fn steps(h: &mut String, r: &Report) {
    h.push_str("<h2 id=\"steps\">Steps</h2>");
    for st in &r.steps {
        let _ = write!(h, "<div class=\"step{}\"><span class=\"n\">{}</span><div>", if st.on { "" } else { " off" }, st.position);
        match &st.command {
            Some(c) => {
                let _ = write!(h, "<code>{}</code>", esc(c));
            }
            None => h.push_str("<span style=\"color:var(--warn)\">Pause</span>"),
        }
        if !st.on {
            h.push_str(" <span class=\"tag\">switched off</span>");
        }
        for (case, words) in &st.last {
            let _ = write!(h, " <span class=\"tag\">{}: {}</span>", esc(case), esc(words));
        }
        if !st.inputs.is_empty() {
            let list: Vec<String> = st.inputs.iter().map(|(n, w)| format!("{}: {}", esc(n), esc(w))).collect();
            let _ = write!(h, "<br><span class=\"mut\">{}</span>", list.join(" · "));
        }
        if !st.settings.is_empty() {
            let list: Vec<String> = st.settings.iter().map(|(n, v)| {
                let changed: Vec<String> = r.cases.iter().flat_map(|c| c.changes.iter().filter(|ch| ch.step == st.position && &ch.field == n).map(move |ch| format!("{}: {}", c.name, json_words(&ch.value)))).collect();
                let extra = if changed.is_empty() { String::new() } else { format!(" <span class=\"d\">({})</span>", esc(&changed.join(", "))) };
                format!("{} {}{extra}", esc(n), esc(v))
            }).collect();
            let _ = write!(h, "<br><span class=\"mut\">{}</span>", list.join(" · "));
        }
        if !st.note.is_empty() {
            let _ = write!(h, "<br><span class=\"note\">{}</span>", esc(&st.note));
        }
        h.push_str("</div></div>");
    }
}

fn inputs(h: &mut String, r: &Report) {
    h.push_str("<h2 id=\"inputs\">Inputs</h2>");
    if r.inputs.is_empty() {
        h.push_str("<div class=\"mut\">Every step reads what an earlier step made.</div>");
        return;
    }
    h.push_str("<div class=\"scroll\"><table><thead><tr><th>Input</th><th class=\"l\">Steps</th><th class=\"l\">Finds now</th></tr></thead><tbody>");
    for i in &r.inputs {
        let _ = write!(h, "<tr><td>{}</td><td class=\"l\">{}</td><td class=\"l\">{}</td></tr>", esc(&i.rule), i.steps.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", "), esc(&i.found));
    }
    h.push_str("</tbody></table></div>");
    let files: Vec<_> = r.inputs.iter().flat_map(|i| i.files.iter()).collect();
    if !files.is_empty() {
        h.push_str("<div class=\"mut\" style=\"margin-top:8px\">Grid files the seam models link, as fingerprinted when linked:</div><div class=\"scroll\"><table><thead><tr><th>File</th><th>Bytes</th><th class=\"l\">Checksum</th><th class=\"l\">Modified</th></tr></thead><tbody>");
        for f in files {
            let (date, time) = civil(f.modified_unix_ms, r.utc_offset_minutes);
            let _ = write!(h, "<tr><td>{}</td>{}<td class=\"l\">{}</td><td class=\"l\">{date} {time}</td></tr>", esc(&f.file), num(Some(f.bytes as f64), 0), f.checksum);
        }
        h.push_str("</tbody></table></div>");
    }
}

/// A cell holding a number, its exact value kept for sorting and CSV.
fn num(v: Option<f64>, dp: usize) -> String {
    match v {
        Some(v) if v.is_finite() => format!("<td data-v=\"{v}\">{}</td>", fixed(v, dp)),
        _ => "<td>–</td>".into(),
    }
}

/// A number to `dp` places with thousands separated by commas.
fn fixed(v: f64, dp: usize) -> String {
    let text = format!("{:.*}", dp, v.abs());
    let (whole, frac) = text.split_once('.').map_or((text.as_str(), ""), |(w, f)| (w, f));
    let mut grouped = String::new();
    for (k, ch) in whole.chars().enumerate() {
        if k > 0 && (whole.len() - k) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let sign = if v < 0.0 && text.chars().any(|c| c.is_ascii_digit() && c != '0') { "-" } else { "" };
    if frac.is_empty() { format!("{sign}{grouped}") } else { format!("{sign}{grouped}.{frac}") }
}

fn signed(v: f64, dp: usize) -> String {
    let text = fixed(v, dp);
    if text.trim_start_matches('-').chars().all(|c| c == '0' || c == '.' || c == ',') { "±0".into() } else if v > 0.0 { format!("+{text}") } else { text }
}

fn int(v: f64) -> String {
    fixed(v, 0)
}

fn tonnes_words(t: Option<f64>) -> String {
    match t {
        Some(t) if t >= 1e6 => format!("{:.2} Mt", t / 1e6),
        Some(t) => format!("{} t", int(t)),
        None => "–".into(),
    }
}

fn when(unix_ms: u64, offset: i32) -> String {
    let (d, t) = civil(unix_ms, offset);
    format!("{d} {t}")
}

fn json_words(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "none".into(),
        other => other.to_string(),
    }
}

fn s(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Text safe inside HTML and attributes.
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_with_commas_and_signs() {
        assert_eq!(fixed(4_820_400.0, 0), "4,820,400");
        assert_eq!(fixed(1.4125, 3), "1.413");
        assert_eq!(fixed(-1234.5, 1), "-1,234.5");
        assert_eq!(fixed(-0.0001, 3), "0.000");
        assert_eq!(fixed(999.0, 0), "999");
        assert_eq!(signed(524.0, 0), "+524");
        assert_eq!(signed(-0.0004, 3), "±0");
        assert_eq!(esc("<a href=\"x\">'&'</a>"), "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;");
    }
}
