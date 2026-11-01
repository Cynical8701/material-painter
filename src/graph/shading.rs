//! Shading graphs: a graph of shading nodes (`@shading` node files) compiled
//! into one preview shader for the 3D view.
//!
//! The graph's Shader Output node decides what each pixel shows. Compiling
//! walks back from it and emits, per node instance, a renamed copy of the
//! node's WGSL (so helper functions of two instances never collide), input
//! functions that call the upstream node (or return the default), `has_*`
//! flags for connected inputs, and the parameters, read from a uniform
//! array so tweaking them doesn't recompile. The result is
//! `fn custom_shade(c: Shade) -> vec3<f32>`, which the view shader calls.

use super::def::{Kind, NodeDef, ParamKind, pack};
use super::{Graph, Library, Node};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

/// The node that ends a shading graph.
pub const OUTPUT: &str = "shade_output";
/// Parameter slots available (one vec4 each); see `viewport::CUSTOM_PARAMS`.
pub const MAX_SLOTS: usize = crate::viewport::CUSTOM_PARAMS;

/// Where a parameter's value goes in the uniform array.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub node: u64,
    pub param: String,
}

/// A compiled preview shader.
#[derive(Clone, Debug)]
pub struct Compiled {
    pub code: String,
    /// Parameter slots in order: slot `i` is `cp[i]` in the shader.
    pub slots: Vec<Slot>,
}

/// Whether a graph is a shading graph (it has shading nodes).
pub fn is_shading(graph: &Graph, lib: &Library) -> bool {
    graph.nodes.iter().any(|n| lib.find(&n.def).is_some_and(|d| d.shading))
}

/// A new shading graph: the painted surface through the PBR look.
pub fn starter(name: &str) -> Graph {
    let mut g = Graph::new(name);
    let pbr = g.add("shade_pbr", [0.0, 0.0]);
    let out = g.add(OUTPUT, [260.0, 0.0]);
    g.connect(pbr, out, "color");
    g
}

/// Ready-made preview shaders to start from.
pub fn presets() -> Vec<Graph> {
    let mut out = Vec::new();

    // Toon: cel bands plus a rim light.
    let mut g = Graph::new("Toon");
    let toon = g.add("shade_toon", [0.0, 0.0]);
    let fres = g.add("shade_fresnel", [0.0, 220.0]);
    let rim = g.add("shade_remap", [220.0, 220.0]);
    let mix = g.add("shade_mix", [460.0, 60.0]);
    let white = g.add("shade_color", [220.0, 380.0]);
    let o = g.add(OUTPUT, [700.0, 60.0]);
    g.connect(fres, rim, "value");
    g.connect(toon, mix, "a");
    g.connect(white, mix, "b");
    g.connect(rim, mix, "mask");
    g.connect(mix, o, "color");
    if let Some(n) = g.node_mut(rim) {
        n.params.insert("low".into(), super::Value::Float(0.55));
        n.params.insert("high".into(), super::Value::Float(0.65));
    }
    if let Some(n) = g.node_mut(white) {
        n.params.insert("strength".into(), super::Value::Float(0.9));
    }
    out.push(g);

    // Clay: a neutral sculpt look that shows form, height and occlusion.
    let mut g = Graph::new("Clay");
    let clay = g.add("shade_color", [0.0, 0.0]);
    let pbr = g.add("shade_pbr", [240.0, 0.0]);
    let rough = g.add("shade_value", [0.0, 160.0]);
    let o = g.add(OUTPUT, [480.0, 0.0]);
    g.connect(clay, pbr, "base");
    g.connect(rough, pbr, "roughness");
    g.connect(pbr, o, "color");
    if let Some(n) = g.node_mut(clay) {
        n.params.insert("color".into(), super::Value::Color([0.42, 0.29, 0.21]));
    }
    if let Some(n) = g.node_mut(rough) {
        n.params.insert("value".into(), super::Value::Float(0.6));
    }
    out.push(g);

    // Unlit: the painted color with no lighting.
    let mut g = Graph::new("Unlit");
    let u = g.add("shade_unlit", [0.0, 0.0]);
    let o = g.add(OUTPUT, [240.0, 0.0]);
    g.connect(u, o, "color");
    out.push(g);

    // PBR + rim: the normal look with a colored edge light.
    let mut g = Graph::new("PBR + Rim Light");
    let pbr = g.add("shade_pbr", [0.0, 0.0]);
    let fres = g.add("shade_fresnel", [0.0, 200.0]);
    let tint = g.add("shade_color", [0.0, 340.0]);
    let rim = g.add("shade_math", [240.0, 260.0]);
    let add = g.add("shade_math", [480.0, 60.0]);
    let o = g.add(OUTPUT, [720.0, 60.0]);
    g.connect(fres, rim, "a");
    g.connect(tint, rim, "b");
    g.connect(pbr, add, "a");
    g.connect(rim, add, "b");
    g.connect(add, o, "color");
    if let Some(n) = g.node_mut(add) {
        n.params.insert("op".into(), super::Value::Enum(0));
    }
    if let Some(n) = g.node_mut(tint) {
        n.params.insert("color".into(), super::Value::Color([0.3, 0.6, 1.0]));
        n.params.insert("strength".into(), super::Value::Float(1.5));
    }
    out.push(g);
    out
}

/// Renames the node's own identifiers (`fn`, `struct`, `const` names) and
/// its `in_*`, `has_*` and `params` to this instance's.
fn rename(body: &str, prefix: &str) -> String {
    let mut own: HashSet<String> = HashSet::new();
    let words: Vec<&str> = body.split(|c: char| !(c.is_alphanumeric() || c == '_')).collect();
    for w in words.windows(2) {
        if matches!(w[0], "fn" | "struct" | "const") && !w[1].is_empty() {
            own.insert(w[1].to_string());
        }
    }
    let mut out = String::with_capacity(body.len() + 256);
    let mut prev_sig = ' '; // last non-space char before the token
    let mut chars = body.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch.is_alphabetic() || ch == '_' {
            let mut end = i + ch.len_utf8();
            while let Some(&(j, c)) = chars.peek() {
                if c.is_alphanumeric() || c == '_' {
                    end = j + c.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let tok = &body[i..end];
            // Field accesses (`c.node`) keep their names.
            let renamed = prev_sig != '.' && (own.contains(tok) || tok == "params" || tok.starts_with("in_") || tok.starts_with("has_"));
            if renamed {
                out.push_str(prefix);
            }
            out.push_str(tok);
            prev_sig = 'a';
        } else {
            out.push(ch);
            if !ch.is_whitespace() {
                prev_sig = ch;
            }
        }
    }
    out
}

fn wgsl_ty(kind: Kind) -> &'static str {
    match kind {
        Kind::Color => "vec4<f32>",
        Kind::Gray => "f32",
    }
}

/// Compiles a shading graph into `custom_shade`.
pub fn compile(graph: &Graph, lib: &Library) -> Result<Compiled, String> {
    let output = graph.nodes.iter().find(|n| n.def == OUTPUT).ok_or("add a Shader Output node: it decides what the 3D view shows")?;
    let by_id: HashMap<u64, &Node> = graph.nodes.iter().map(|n| (n.id, n)).collect();

    // Nodes the output depends on, upstream first.
    let mut order: Vec<u64> = Vec::new();
    let mut seen: HashSet<u64> = HashSet::new();
    fn visit(id: u64, by_id: &HashMap<u64, &Node>, seen: &mut HashSet<u64>, order: &mut Vec<u64>) {
        if !seen.insert(id) {
            return;
        }
        if let Some(n) = by_id.get(&id) {
            let mut ups: Vec<u64> = n.links.values().copied().collect();
            ups.sort_unstable();
            for up in ups {
                visit(up, by_id, seen, order);
            }
        }
        order.push(id);
    }
    visit(output.id, &by_id, &mut seen, &mut order);

    let mut code = String::new();
    let mut slots: Vec<Slot> = Vec::new();
    let mut init = String::new();
    for &id in &order {
        let node = by_id[&id];
        let def: &NodeDef = lib.find(&node.def).ok_or_else(|| format!("unknown node '{}'", node.def))?;
        if !def.shading {
            return Err(format!("'{}' is a texture node; preview shaders take shading nodes only", def.name));
        }
        let p = format!("n{id}_");
        let _ = writeln!(code, "\n// ---- {} (node {id})", def.name);
        // Parameters: a private struct filled from the uniform array.
        let mut fields = String::from("    _pad: f32,\n");
        for prm in &def.params {
            let (ty, read) = match prm.kind {
                ParamKind::Float { .. } => ("f32", "cp[{s}].x"),
                ParamKind::Int { .. } | ParamKind::Enum(_) => ("i32", "i32(round(cp[{s}].x))"),
                ParamKind::Bool => ("bool", "cp[{s}].x > 0.5"),
                ParamKind::Color => ("vec3<f32>", "cp[{s}].xyz"),
                ParamKind::Image | ParamKind::MeshMap => continue,
            };
            let s = slots.len();
            if s >= MAX_SLOTS {
                return Err(format!("too many parameters (at most {MAX_SLOTS} in one preview shader)"));
            }
            slots.push(Slot { node: id, param: prm.name.clone() });
            let _ = writeln!(fields, "    {}: {ty},", prm.name);
            let _ = writeln!(init, "    {p}params.{} = {};", prm.name, read.replace("{s}", &s.to_string()));
        }
        let _ = write!(code, "struct {p}Params {{\n{fields}}};\nvar<private> {p}params: {p}Params;\n");
        // Inputs: the upstream node's value, or the default while unconnected.
        for (k, inp) in def.inputs.iter().enumerate() {
            let ty = wgsl_ty(inp.kind);
            let up = node.links.get(&inp.name).and_then(|u| Some((*u, lib.find(&by_id.get(u)?.def)?)));
            let value = match up {
                Some((u, up_def)) => {
                    let v = format!("n{u}_node(c)");
                    match (inp.kind, up_def.output) {
                        (Kind::Color, Kind::Color) => v,
                        (Kind::Color, Kind::Gray) => format!("vec4<f32>({v}.rrr, 1.0)"),
                        (Kind::Gray, _) => format!("{v}.r"),
                    }
                }
                None => {
                    let d = inp.default;
                    match inp.kind {
                        Kind::Color => format!("vec4<f32>({:?}, {:?}, {:?}, {:?})", d[0], d[1], d[2], d[3]),
                        Kind::Gray => format!("{:?}", d[0]),
                    }
                }
            };
            let _ = write!(
                code,
                "fn {p}in_{n}(c: Shade) -> {ty} {{\n    return {value};\n}}\nfn {p}has_{n}() -> bool {{\n    return {};\n}}\n",
                up.is_some(),
                n = inp.name
            );
            let _ = k;
        }
        code.push_str(&rename(&def.body, &p));
        code.push('\n');
    }
    let out_node = output.id;
    let _ = write!(
        code,
        "\nfn custom_shade(c: Shade) -> vec3<f32> {{\n{init}    return n{out_node}_node(c).rgb;\n}}\n"
    );
    Ok(Compiled { code, slots })
}

/// The values of a compiled shader's parameters, in slot order.
pub fn param_values(compiled: &Compiled, graph: &Graph, lib: &Library) -> Vec<[f32; 4]> {
    compiled
        .slots
        .iter()
        .map(|s| {
            graph
                .nodes
                .iter()
                .find(|n| n.id == s.node)
                .and_then(|n| lib.value(n, &s.param))
                .map_or([0.0; 4], |v| pack(&v))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renaming_keeps_fields_and_prefixes_own_names() {
        let body = "fn helper(x: f32) -> f32 { return x; }\nfn node(c: Shade) -> vec4<f32> { let a = in_base(c); if (has_base()) { return vec4<f32>(helper(params.k) + c.s.rough); } return a; }";
        let r = rename(body, "n7_");
        assert!(r.contains("fn n7_helper(") && r.contains("n7_helper(n7_params.k)"), "{r}");
        assert!(r.contains("fn n7_node(") && r.contains("n7_in_base(c)") && r.contains("n7_has_base()"), "{r}");
        assert!(r.contains("c.s.rough"), "fields stay: {r}");
    }

    #[test]
    fn every_preset_compiles_to_one_function_with_its_parameters() {
        let lib = Library::load(None);
        let shading: Vec<&NodeDef> = lib.defs.iter().filter(|d| d.shading).collect();
        assert!(shading.len() >= 20, "built-in shading nodes load");
        for g in presets().iter().chain([&starter("Starter")]) {
            assert!(is_shading(g, &lib));
            let c = compile(g, &lib).unwrap_or_else(|e| panic!("{}: {e}", g.name));
            assert!(c.code.contains("fn custom_shade(c: Shade) -> vec3<f32>"));
            assert_eq!(param_values(&c, g, &lib).len(), c.slots.len());
        }
        // No output, no shader.
        let mut g = Graph::new("x");
        g.add("shade_pbr", [0.0, 0.0]);
        assert!(compile(&g, &lib).is_err());
    }
}
