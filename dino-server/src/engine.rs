use std::collections::HashMap;

use anyhow::Result;
use axum::{body::Body, response::Response};
use dino_macros::{FromJs, IntoJs};
use rquickjs::{Context, Function, IntoJs, Object, Promise, Runtime};
use typed_builder::TypedBuilder;

#[allow(unused)]
pub struct JsWorker {
    rt: Runtime,
    ctx: Context,
}

#[derive(Debug, TypedBuilder, IntoJs)]
pub struct Req {
    #[builder(default)]
    pub headers: HashMap<String, String>,
    #[builder(default)]
    pub query: HashMap<String, String>,
    #[builder(default)]
    pub params: HashMap<String, String>,
    #[builder(default)]
    pub body: Option<String>,
    #[builder(setter(into))]
    pub url: String,
    #[builder(setter(into))]
    pub method: String,
}

#[derive(Debug, FromJs)]
#[allow(unused)]
pub struct Resp {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

fn print(msg: String) {
    println!("{msg}");
}

impl JsWorker {
    pub fn try_new(module: &str) -> Result<Self> {
        let rt = Runtime::new()?;
        let ctx = Context::full(&rt)?;

        ctx.with(|ctx| {
            let global = ctx.globals();
            let ret: Object = ctx.eval(module)?;
            global.set("handlers", ret)?;

            let func = Function::new(ctx.clone(), print)?.with_name("print")?;
            global.set("print", func)?;

            Ok::<_, anyhow::Error>(())
        })?;

        Ok(Self { rt, ctx })
    }

    pub fn run(&self, name: &str, req: Req) -> Result<Resp> {
        self.ctx.with(|ctx| {
            let global = ctx.globals();
            let handlers: Object = global.get("handlers")?;

            let fun: Function = handlers.get(name)?;
            let v: Promise = fun.call((req,))?;

            Ok::<_, anyhow::Error>(v.finish::<Resp>()?)
        })
    }
}

impl From<Resp> for Response {
    fn from(res: Resp) -> Self {
        let mut builder = Response::builder().status(res.status);
        for (k, v) in res.headers {
            builder = builder.header(k, v);
        }
        if let Some(body) = res.body {
            builder.body(body.into()).unwrap()
        } else {
            builder.body(Body::empty()).unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_worker_should_run() {
        let code = r#"
         (function(){
         async function hello(req){
             return {
                 status:200,
                 headers:{
                     "content-type":"application/json"
                 },
                 body: JSON.stringify(req),
             };
         }
         return{hello:hello};
     })();
     "#;
        let req = Req::builder()
            .method("GET")
            .url("https://www.baidu.com")
            .headers(HashMap::new())
            .build();

        let worker = JsWorker::try_new(code).unwrap();
        let resp = worker.run("hello", req).unwrap();
        println!("{:?}", resp);
        assert_eq!(resp.status, 200);
    }
}
