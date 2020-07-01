use async_std::fs::read_to_string;
use liquid::{Object, Template};
use std::{collections::HashMap, error::Error};
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

    let templates = compile_templates(&[
        "./templates/index.html.liquid",
        "./templates/style.css.liquid",
        "./templates/main.js.liquid",
    ])
    .await?;
    log::info!("{} templates compiled", templates.len());

    let mut app = tide::with_state(State { templates });

    app.at("/").get(|req: Request<State>| async move {
        serve_template(&req.state().templates, "index.html", mimes::html())
            .await
            .for_tide()
    });

    app.at("/style.css").get(|req: Request<State>| async move {
        serve_template(&req.state().templates, "style.css", mimes::css())
            .await
            .for_tide()
    });

    app.at("/main.js").get(|req: Request<State>| async move {
        serve_template(&req.state().templates, "main.js", mimes::js())
            .await
            .for_tide()
    });

    app.at("/upload")
        .post(|mut req: Request<State>| async move {
            let mut res = Response::new(StatusCode::Ok);
            let body = req.body_bytes().await?;
            let s = base64::encode(body);
            let src = format!("data:image/jpeg;base64,{}", s);
            res.set_body(format!(
                r#"
        {{
            "src": {:?}
        }}
        "#,
                src
            ));
            Ok(res)
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

trait ForTide {
    fn for_tide(self) -> Result<tide::Response, tide::Error>;
}

impl ForTide for Result<tide::Response, Box<dyn Error>> {
    fn for_tide(self) -> Result<Response, tide::Error> {
        self.map_err(|e| {
            log::error!("While serving template: {}", e);
            tide::Error::from_str(
                StatusCode::InternalServerError,
                "Something went wrong, sorry!",
            )
        })
    }
}

mod mimes {
    use std::str::FromStr;
    use tide::http::Mime;

    pub(crate) fn html() -> Mime {
        Mime::from_str("text/html; charset=utf-8").unwrap()
    }

    pub(crate) fn css() -> Mime {
        Mime::from_str("text/css; charset=utf-8").unwrap()
    }

    pub(crate) fn js() -> Mime {
        Mime::from_str("text/javascript; charset=utf-8").unwrap()
    }
}

async fn serve_template(
    templates: &TemplateMap,
    name: &str,
    mime: Mime,
) -> Result<Response, Box<dyn Error>> {
    let template = templates
        .get(name)
        .ok_or_else(|| TemplateError::TemplateNotFound(name.to_string()))?;
    let globals: Object = Default::default();
    let markup = template.render(&globals)?;
    let mut res = Response::new(StatusCode::Ok);
    res.set_content_type(mime);
    res.set_body(markup);
    Ok(res)
}
