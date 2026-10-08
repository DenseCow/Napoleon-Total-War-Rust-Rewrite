//! re_tools: read-only reverse-engineering helpers for Napoleon: Total War binaries.
//! Subcommands:
//!   pe <file>...                     PE header/section/import/export/debug/rich report
//!   strings <exe> <outdir>           ASCII+UTF16 strings (min 6) + categorisation
//!   rtti <exe> <outdir>              MSVC RTTI classes, vtables, hierarchies
//!   xref <exe> <outfile> sub|exact <needle>...   code refs to strings + function summaries
//!   func <exe> <hexva>...            summarise function containing VA + its callers
mod classes;
mod demangle;
mod pe;
mod rtti;
mod strings;
mod xref;
mod tweak;
mod luabind;
mod kvmap;
mod dbmap;
mod dbschema;
mod vdecl;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 { eprintln!("usage: re_tools pe|strings|rtti|xref|func ..."); std::process::exit(2); }
    match a[1].as_str() {
        "pe" => { for f in &a[2..] { print!("{}", pe::report(f)); } }
        "strings" => { let p = pe::Pe::load(&a[2]).unwrap(); strings::run(&p, &a[3]); }
        "classes" => { let p = pe::Pe::load(&a[2]).unwrap(); classes::run(&p, &a[3]); }
        "rtti" => { let p = pe::Pe::load(&a[2]).unwrap(); rtti::run(&p, &a[3]); }
        "xref" => { let p = pe::Pe::load(&a[2]).unwrap(); xref::run(&p, &a[3], a[4] == "exact", &a[5..]); }
        "imm" => { let p = pe::Pe::load(&a[2]).unwrap(); let v: Vec<u32> = a[3..].iter().map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()).collect(); xref::run_imm(&p, &v); }
        "iat" => { let p = pe::Pe::load(&a[2]).unwrap(); let v: Vec<u32> = a[3..].iter().map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()).collect(); xref::run_iat(&p, &v); }
        "tweak" => { let p = pe::Pe::load(&a[2]).unwrap(); tweak::run(&p, &a[3]); }
        "impref" => { let p = pe::Pe::load(&a[2]).unwrap(); xref::run_impref(&p, &a[3..]); }
        "luabind" => { let p = pe::Pe::load(&a[2]).unwrap(); luabind::run(&p, &a[3]); }
        "tweakuse" => { let p = pe::Pe::load(&a[2]).unwrap(); let v: Vec<u32> = a[4..].iter().map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()).collect(); xref::run_tweakuse(&p, &a[3], &v); }
        "kvmap" => { let p = pe::Pe::load(&a[2]).unwrap(); let v: Vec<(String,u32)> = a[4..].chunks(2).map(|c| (c[0].clone(), u32::from_str_radix(c[1].trim_start_matches("0x"), 16).unwrap())).collect(); kvmap::run(&p, &a[3], &v); }
        "kvuse" => { let p = pe::Pe::load(&a[2]).unwrap(); kvmap::run_use(&p, &a[3], &a[4]); }
        "getteruse" => { let p = pe::Pe::load(&a[2]).unwrap(); kvmap::run_getteruse(&p, &a[3], &a[4]); }
        "findptr" => { let p = pe::Pe::load(&a[2]).unwrap(); let v: Vec<u32> = a[3..].iter().map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()).collect(); xref::run_findptr(&p, &v); }
        "dbmap" => { let p = pe::Pe::load(&a[2]).unwrap(); dbmap::run(&p, &a[3]); }
        "dbschema" => { dbschema::run(&a[2], &a[3], &a[4], &a[5..].to_vec()); }
        "storescan" => { let p = pe::Pe::load(&a[2]).unwrap(); let v: Vec<u32> = a[3..].iter().map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()).collect(); xref::run_storescan(&p, &v); }
        "vdecl" => { let p = pe::Pe::load(&a[2]).unwrap(); vdecl::run(&p, &a[3]); }
        "vcall" => { let p = pe::Pe::load(&a[2]).unwrap(); xref::run_vcall(&p, u32::from_str_radix(a[3].trim_start_matches("0x"), 16).unwrap()); }
        "func" => {
            let p = pe::Pe::load(&a[2]).unwrap();
            let v: Vec<u32> = a[3..].iter().map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()).collect();
            xref::run_func(&p, &v);
        }
        _ => { eprintln!("unknown subcommand"); std::process::exit(2); }
    }
}
