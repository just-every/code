use once_cell::sync::Lazy;
use std::collections::HashMap;

// circularDeps is the list of types that can cause circular dependency
// issues.
static CIRCULAR_DEPS: Lazy<HashMap<&'static str, bool>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("browser.browsercontextid", true);
    m.insert("dom.backendnodeid", true);
    m.insert("dom.backendnode", true);
    m.insert("dom.nodeid", true);
    m.insert("dom.node", true);
    m.insert("dom.nodetype", true);
    m.insert("dom.pseudotype", true);
    m.insert("dom.rgba", true);
    m.insert("dom.shadowroottype", true);
    m.insert("network.loaderid", true);
    m.insert("network.monotonictime", true);
    m.insert("network.timesinceepoch", true);
    m.insert("page.frameid", true);
    m.insert("page.frame", true);
    m
});

// returns whether or not a type will cause circular dependency
// issues.
pub(crate) fn is_circular_dep(dty: &str, ty_str: &str) -> bool {
    CIRCULAR_DEPS
        .get(format!("{}.{}", dty.to_lowercase(), ty_str.to_lowercase()).as_str())
        .cloned()
        .unwrap_or_default()
}
