// Tells cargo to recompile when a migration file is added/changed,
// because `sqlx::migrate!()` embeds them at compile time.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
