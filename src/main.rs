use liquid::{Object, Template};
use std::{collections::HashMap, error::Error, net::SocketAddr};
use tide::{http::Mime, Request, Response, StatusCode};
use warp::Filter;

use tokio::{fs::read_to_string, sync::RwLock};

use image::{imageops::FilterType, jpeg::JPEGEncoder, DynamicImage, GenericImageView};
use rand::Rng;
use serde::Serialize;
use ulid::Ulid;

#[derive(Serialize)]
struct UploadResponse<'a> {
    src: &'a str,
}

struct Image {
    mime: Mime,
    contents: Vec<u8>,
}

struct State {
    templates: TemplateMap,
    images: RwLock<HashMap<Ulid, Image>>,
}

pub const JPEG_QUALITY: u8 = 25;

trait BitCrush: Sized {
    type Error;

    fn bitcrush(self) -> Result<Self, Self::Error>;
}

impl BitCrush for DynamicImage {
    type Error = image::ImageError;

    fn bitcrush(self) -> Result<Self, Self::Error> {
        let mut current = self;
        let (orig_w, orig_h) = current.dimensions();
        let mut rng = rand::thread_rng();
        let (temp_w, temp_h) = (
            rng.gen_range(orig_w / 2, orig_w * 2),
            rng.gen_range(orig_h / 2, orig_h * 2),
        );

        let mut out: Vec<u8> = Default::default();
        for _ in 0..2 {
            current = current
                .resize_exact(temp_w, temp_h, FilterType::Nearest)
                .rotate180()
                .huerotate(180);
            out.clear();
            {
                let mut encoder = JPEGEncoder::new_with_quality(&mut out, rng.gen_range(10, 30));
                encoder.encode_image(&current)?;
            }
            current = image::load_from_memory_with_format(&out[..], image::ImageFormat::Jpeg)?
                .resize_exact(orig_w, orig_h, FilterType::Nearest)
                .brighten(1);
        }
        Ok(current)
    }
}

trait MimeAware {
    fn content_type(self, mime: Mime) -> Self;
}

impl MimeAware for http::response::Builder {
    fn content_type(self, mime: Mime) -> Self {
        self.header("content-type", mime.to_string())
    }
}

#[tokio::main]
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

    let state = State {
        templates,
        images: Default::default(),
    };

    let index = warp::path::end().and(warp::filters::method::get()).map(|| {
        http::Response::builder()
            .content_type(mimes::html())
            .body("<html><body><p>I do <em>not</em> miss XHTML.</p></body></html>")
    });
    let addr: SocketAddr = "127.0.0.1:3000".parse()?;
    warp::serve(index).run(addr).await;
    Ok(())

    // let mut app = tide::with_state(state);

    // app.at("/").get(|req: Request<State>| async move {
    //     serve_template(&req.state().templates, "index.html", mimes::html())
    //         .await
    //         .for_tide()
    // });

    // app.at("/style.css").get(|req: Request<State>| async move {
    //     serve_template(&req.state().templates, "style.css", mimes::css())
    //         .await
    //         .for_tide()
    // });

    // app.at("/main.js").get(|req: Request<State>| async move {
    //     serve_template(&req.state().templates, "main.js", mimes::js())
    //         .await
    //         .for_tide()
    // });

    // app.at("/upload")
    //     .post(|mut req: Request<State>| async move {
    //         let body = req.body_bytes().await?;
    //         let img = image::load_from_memory(&body[..])?.bitcrush()?;
    //         let mut output: Vec<u8> = Default::default();
    //         let mut encoder = JPEGEncoder::new_with_quality(&mut output, JPEG_QUALITY);
    //         encoder.encode_image(&img)?;

    //         let id = Ulid::new();
    //         let src = format!("/images/{}", id);

    //         let img = Image {
    //             mime: tide::http::mime::JPEG,
    //             contents: output,
    //         };
    //         {
    //             let mut images = req.state().images.write().await;
    //             images.insert(id, img);
    //         }

    //         let mut res = Response::new(StatusCode::Ok);
    //         res.set_content_type(tide::http::mime::JSON);
    //         res.set_body(tide::Body::from_json(&UploadResponse { src: &src })?);
    //         Ok(res)
    //     });

    // app.at("/images/:name")
    //     .get(|req: Request<State>| async { serve_image(req).await.for_tide() });

    // app.listen("localhost:3000").await?;
    // Ok(())
}

pub type TemplateMap = HashMap<String, Template>;

#[derive(Debug, thiserror::Error)]
enum TemplateError {
    #[error("invalid template path: {0}")]
    InvalidTemplatePath(String),
    #[error("template not found: {0}")]
    TemplateNotFound(String),
}

#[derive(Debug, thiserror::Error)]
enum ImageError {
    #[error("invalid image ID")]
    InvalidID,
}

async fn serve_image(req: Request<State>) -> Result<Response, Box<dyn Error>> {
    let id: Ulid = req.param("name").map_err(|_| ImageError::InvalidID)?;

    let images = req.state().images.read().await;
    if let Some(img) = images.get(&id) {
        let mut res = Response::new(200);
        res.set_content_type(img.mime.clone());
        res.set_body(&img.contents[..]);
        Ok(res)
    } else {
        Ok(Response::new(StatusCode::NotFound))
    }
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
