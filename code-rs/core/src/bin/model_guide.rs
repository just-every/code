use code_core::agent_defaults::agent_model_specs;

fn main() {
    for spec in agent_model_specs() {
        println!("- `{}`: {}", spec.slug, spec.description);
    }
}
