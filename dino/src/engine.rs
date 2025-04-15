use std::collections::HashMap;

use anyhow::Result;
use rquickjs::{Context, Ctx, FromJs, Function, IntoJs, Object, Promise, Runtime, Value};
use typed_builder::TypedBuilder;

#[allow(unused)]
pub struct JsWorker {
    rt: Runtime,
    ctx: Context,
}

#[derive(Debug, TypedBuilder)]
pub struct Req {
    pub headers: HashMap<String, String>,
    #[builder(default, setter(strip_option))]
    pub body: Option<String>,
    #[builder(setter(into))]
    pub url: String,
    #[builder(setter(into))]
    pub method: String,
}

#[derive(Debug)]
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

impl<'js> IntoJs<'js> for Req {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        let obj = Object::new(ctx.clone())?;

        obj.set("headers", self.headers)?;
        obj.set("body", self.body)?;
        obj.set("url", self.url)?;
        obj.set("method", self.method)?;

        Ok(obj.into())
    }
}

impl<'js> FromJs<'js> for Resp {
    fn from_js(_ctx: &Ctx<'js>, value: Value<'js>) -> rquickjs::Result<Self> {
        let obj = value.into_object().unwrap();

        let headers: HashMap<String, String> = obj.get("headers")?;
        let body: Option<String> = obj.get("body")?;
        let status: u16 = obj.get("status")?;

        Ok(Self {
            headers,
            body,
            status,
        })
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
