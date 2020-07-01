use async_std::fs::read_to_string;
use liquid::{Object, Template};
use std::{collections::HashMap, error::Error, str::FromStr};
use tide::{http::Mime, Request, Response, StatusCode};

struct State {
    templates: TemplateMap,
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "info");
    }
    pretty_env_logger::init();

    let templates = compile_templates(&["./templates/index.html.liquid"]).await?;
    log::info!("{} templates compiled", templates.len());

    let mut app = tide::with_state(State { templates });
    app.at("/").get(|req: Request<State>| async move {
        log::info!("Serving /");
        let name = "index.html";
        serve_template(&req.state().templates, name)
            .await
            .map_err(|e| {
                log::error!("While serving template: {}", e);
                tide::Error::from_str(
                    StatusCode::InternalServerError,
                    "Something went wrong, sorry!",
                )
            })
    });
    app.listen("localhost:3000").await?;
    Ok(())
}

pub type TemplateMap = HashMap<String, Template>;

#[derive(Debug, thiserror::Error)]
enum TemplateError {
    #[error("invalid template path: {0}")]
    InvalidTemplatePath(String),
    #[error("template not found: {0}")]
    TemplateNotFound(String),
}

async fn compile_templates(paths: &[&str]) -> Result<TemplateMap, Box<dyn Error>> {
    let compiler = liquid::ParserBuilder::with_stdlib().build()?;

    let mut map = TemplateMap::new();
    for path in paths {
        let name = path
            .split('/')
            .last()
            .map(|name| name.trim_end_matches(".liquid"))
            .ok_or_else(|| TemplateError::InvalidTemplatePath(path.to_string()))?;
        let source = read_to_string(path).await?;
        let template = compiler.parse(&source)?;
        map.insert(name.to_string(), template);
    }
    Ok(map)
}

async fn serve_template(templates: &TemplateMap, name: &str) -> Result<Response, Box<dyn Error>> {
    let template = templates
        .get(name)
        .ok_or_else(|| TemplateError::TemplateNotFound(name.to_string()))?;
    let globals: Object = Default::default();
    let markup = template.render(&globals)?;
    let mut res = Response::new(StatusCode::Ok);
    res.set_content_type(Mime::from_str("text/html; charset=utf-8").unwrap());
    res.set_body(markup);
    Ok(res)
}
