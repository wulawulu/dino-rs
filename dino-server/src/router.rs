use anyhow::Result;
use axum::http::Method;
use std::sync::Arc;

use arc_swap::ArcSwap;
use matchit::{Match, Router};

use crate::config::ProjectRoutes;

#[derive(Clone)]
pub struct SwappableAppRouter {
    pub routes: Arc<ArcSwap<AppRouter>>,
}

#[derive(Clone)]
pub struct AppRouter {
    pub routes: Router<MethodRoute>,
    pub code: String,
}

#[derive(Debug, Default, Clone)]
pub struct MethodRoute {
    get: Option<String>,
    post: Option<String>,
    put: Option<String>,
    delete: Option<String>,
    patch: Option<String>,
    head: Option<String>,
    options: Option<String>,
    connect: Option<String>,
    trace: Option<String>,
}

impl SwappableAppRouter {
    pub fn try_new(code: impl Into<String>, routes: ProjectRoutes) -> Result<Self> {
        let router = Self::get_router(routes)?;
        Ok(Self {
            routes: Arc::new(ArcSwap::from_pointee(AppRouter {
                routes: router,
                code: code.into(),
            })),
        })
    }

    pub fn swap(&self, code: impl Into<String>, routes: ProjectRoutes) -> Result<()> {
        let router = Self::get_router(routes)?;
        self.routes.store(Arc::new(AppRouter {
            routes: router,
            code: code.into(),
        }));
        Ok(())
    }

    pub fn load(&self) -> AppRouter {
        AppRouter {
            routes: self.routes.load_full().routes.clone(),
            code: self.routes.load_full().code.clone(),
        }
    }

    fn get_router(routes: ProjectRoutes) -> Result<Router<MethodRoute>> {
        let mut router = Router::new();
        for (path, methods) in routes {
            let mut method_route = MethodRoute::default();
            for method in methods {
                match method.method {
                    Method::GET => method_route.get = Some(method.handler),
                    Method::POST => method_route.post = Some(method.handler),
                    Method::PUT => method_route.put = Some(method.handler),
                    Method::DELETE => method_route.delete = Some(method.handler),
                    Method::PATCH => method_route.patch = Some(method.handler),
                    Method::HEAD => method_route.head = Some(method.handler),
                    Method::OPTIONS => method_route.options = Some(method.handler),
                    Method::CONNECT => method_route.connect = Some(method.handler),
                    Method::TRACE => method_route.trace = Some(method.handler),
                    _ => unreachable!(),
                }
            }
            router.insert(path, method_route)?;
        }
        Ok(router)
    }
}

impl AppRouter {
    #[allow(elided_named_lifetimes)]
    pub fn match_it<'m, 'p>(&'m self, method: Method, path: &'p str) -> Result<Match<&'m str>>
    where
        'p: 'm,
    {
        let Ok(ret) = self.routes.at(path) else {
            return Err(anyhow::anyhow!("No route found for path: {}", path));
        };
        let handler = match method {
            Method::GET => ret.value.get.as_deref(),
            Method::POST => ret.value.post.as_deref(),
            Method::PUT => ret.value.put.as_deref(),
            Method::DELETE => ret.value.delete.as_deref(),
            Method::PATCH => ret.value.patch.as_deref(),
            Method::HEAD => ret.value.head.as_deref(),
            Method::OPTIONS => ret.value.options.as_deref(),
            Method::CONNECT => ret.value.connect.as_deref(),
            Method::TRACE => ret.value.trace.as_deref(),
            _ => unreachable!(),
        }
        .ok_or_else(|| anyhow::anyhow!("No handler found for method: {}", method))?;

        Ok(Match {
            value: handler,
            params: ret.params,
        })
    }
}
#[cfg(test)]
mod tests {
    use crate::config::ProjectConfig;

    use super::*;

    #[test]
    fn app_router_match_should_work() {
        let config: ProjectConfig =
            ProjectConfig::load("./fixtures/config.yml").expect("cannot find config file");
        let router = SwappableAppRouter::try_new("", config.routes).unwrap();
        let app_router = router.load();
        let match_result = app_router.match_it(Method::GET, "/api/hello/123").unwrap();
        assert_eq!(match_result.value, "hello");
        assert_eq!(match_result.params.get("id"), Some("123"));

        let match_result = app_router.match_it(Method::POST, "/api/goodbye/2").unwrap();
        assert_eq!(match_result.value, "hello");
        assert_eq!(match_result.params.get("id"), Some("2"));
        assert_eq!(match_result.params.get("name"), Some("goodbye"));
    }

    #[test]
    fn app_router_swap_should_work() {
        let config: ProjectConfig =
            ProjectConfig::load("./fixtures/config.yml").expect("cannot find config file");
        let router = SwappableAppRouter::try_new("", config.routes).unwrap();
        let app_router = router.load();
        let m = app_router.match_it(Method::GET, "/api/hello/1").unwrap();
        assert_eq!(m.value, "hello");

        let new_config = include_str!("../fixtures/config1.yml");
        let new_config: ProjectConfig = serde_yaml::from_str(new_config).unwrap();
        router.swap("", new_config.routes).unwrap();
        let app_router = router.load();
        let m = app_router.match_it(Method::GET, "/api/hello/1").unwrap();
        assert_eq!(m.value, "hello2");

        let m = app_router.match_it(Method::POST, "/api/goodbye/2").unwrap();
        assert_eq!(m.value, "handler2");
    }
}
