use anyhow::Result;

pub fn render_template(name: &str, ctx: &serde_json::Value) -> Result<String> {
    use tinytemplate::TinyTemplate;
    let base = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("cannot resolve config dir"))?;
    let path = base
        .join("rusty-cli")
        .join("templates")
        .join(format!("{}.tmpl", name));
    let tpl = std::fs::read_to_string(&path)?;
    let mut tt = TinyTemplate::new();
    tt.add_template(name, &tpl)?;
    let rendered = tt.render(name, ctx)?;
    Ok(rendered)
}
