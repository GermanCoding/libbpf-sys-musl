// build.rs

use std::env;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::os::unix::prelude::*;
use std::path;
use std::process;

#[cfg(feature = "bindgen")]
fn generate_bindings(src_dir: path::PathBuf) {
    use std::collections::HashSet;

    #[derive(Debug)]
    struct IgnoreMacros(HashSet<&'static str>);

    impl bindgen::callbacks::ParseCallbacks for IgnoreMacros {
        fn will_parse_macro(&self, name: &str) -> bindgen::callbacks::MacroParsingBehavior {
            if self.0.contains(name) {
                bindgen::callbacks::MacroParsingBehavior::Ignore
            } else {
                bindgen::callbacks::MacroParsingBehavior::Default
            }
        }
    }

    let ignored_macros = IgnoreMacros(
        vec![
            "BTF_KIND_FUNC",
            "BTF_KIND_FUNC_PROTO",
            "BTF_KIND_VAR",
            "BTF_KIND_DATASEC",
            "BTF_KIND_FLOAT",
            "BTF_KIND_DECL_TAG",
            "BTF_KIND_TYPE_TAG",
        ]
            .into_iter()
            .collect(),
    );

    bindgen::Builder::default()
        .derive_default(true)
        .explicit_padding(true)
        .default_enum_style(bindgen::EnumVariation::Consts)
        .prepend_enum_name(false)
        .layout_tests(false)
        .generate_comments(false)
        .emit_builtins()
        .allowlist_function("bpf_.+")
        .allowlist_function("btf_.+")
        .allowlist_function("libbpf_.+")
        .allowlist_function("xsk_.+")
        .allowlist_function("_xsk_.+")
        .allowlist_function("perf_buffer_.+")
        .allowlist_function("perf_event_.+")
        .allowlist_function("ring_buffer_.+")
        .allowlist_function("vdprintf")
        .allowlist_type("bpf_.+")
        .allowlist_type("btf_.+")
        .allowlist_type("xdp_.+")
        .allowlist_type("xsk_.+")
        .allowlist_type("perf_event_.+")
        .allowlist_type("perf_sample_.+")
        .allowlist_var("BPF_.+")
        .allowlist_var("BTF_.+")
        .allowlist_var("XSK_.+")
        .allowlist_var("XDP_.+")
        .allowlist_var("PERF_RECORD_.+")
        .parse_callbacks(Box::new(ignored_macros))
        .header("bindings.h")
        .clang_arg(format!("-I{}", src_dir.join("libbpf/include").display()))
        .clang_arg(format!(
            "-I{}",
            src_dir.join("libbpf/include/uapi").display()
        ))
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(&src_dir.join("src/bindings.rs"))
        .expect("Couldn't write bindings");
}

#[cfg(not(feature = "bindgen"))]
fn generate_bindings(_: path::PathBuf) {}

fn main() {
    let src_dir = path::PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = path::PathBuf::from(env::var_os("OUT_DIR").unwrap());

    generate_bindings(src_dir.clone());

    if cfg!(feature = "novendor") {
        let libbpf = pkg_config::Config::new()
            .atleast_version(&format!(
                "{}.{}.{}",
                env!("CARGO_PKG_VERSION_MAJOR"),
                env!("CARGO_PKG_VERSION_MINOR"),
                env!("CARGO_PKG_VERSION_PATCH")
            ))
            .probe("libbpf")
            .unwrap();

        cc::Build::new()
            .file("bindings.c")
            .includes(&libbpf.include_paths)
            .define("__LIBBPF_SYS_NOVENDOR", None)
            .out_dir(&out_dir)
            .compile("bindings");
    } else {
        if let Err(_) = process::Command::new("make").status() {
            panic!("make is required to compile libbpf-sys using the vendored copy of libbpf");
        }

        if let Err(_) = process::Command::new("pkg-config").status() {
            panic!(
                "pkg-config is required to compile libbpf-sys using the vendored copy of libbpf"
            );
        }

        let compiler = match cc::Build::new().try_get_compiler() {
            Ok(compiler) => compiler,
            Err(_) => panic!(
                "a C compiler is required to compile libbpf-sys using the vendored copy of libbpf"
            ),
        };

        // create obj_dir if it doesn't exist
        let obj_dir = path::PathBuf::from(&out_dir.join("obj").into_os_string());
        let _ = fs::create_dir(&obj_dir);
        let mut flags = compiler.cflags_env();
        flags.push(" -I");
        flags.push(src_dir.join("include").into_os_string());

        if env::var("CARGO_CFG_TARGET_ENV").unwrap().contains("musl") {
            // Externally populate include directory + build libelf with dependencies
            let triple = env::var("CARGO_CFG_TARGET_ARCH").unwrap()
                + "-" + &env::var("CARGO_CFG_TARGET_OS").unwrap()
                + "-" + &env::var("CARGO_CFG_TARGET_ENV").unwrap();
            let status = process::Command::new("./build-libelf.sh")
                .arg(triple)
                .env("CC", compiler.path())
                .current_dir(&src_dir)
                .status()
                .expect("Could not execute builf-libelf.sh");

            assert!(status.success(), "Building libelf failed. Perhaps you are missing required tools?");
        }

        let status = process::Command::new("make")
            .arg("install")
            .env("BUILD_STATIC_ONLY", "y")
            .env("PREFIX", "/")
            .env("LIBDIR", "")
            .env("OBJDIR", &obj_dir)
            .env("DESTDIR", &out_dir)
            .env("CC", compiler.path())
            .env("CFLAGS", flags)
            .current_dir(&src_dir.join("libbpf/src"))
            .status()
            .expect("could not execute make");

        assert!(status.success(), "make failed");

        let status = process::Command::new("make")
            .arg("clean")
            .current_dir(&src_dir.join("libbpf/src"))
            .status()
            .expect("could not execute make");

        assert!(status.success(), "make failed");

        cc::Build::new()
            .file("bindings.c")
            .include(&src_dir.join("libbpf/include"))
            .include(&src_dir.join("libbpf/include/uapi"))
            .include(&src_dir.join("include"))
            .out_dir(&out_dir)
            .compile("bindings");

        io::stdout()
            .write_all("cargo:rustc-link-search=native=".as_bytes())
            .unwrap();
        io::stdout()
            .write_all(out_dir.as_os_str().as_bytes())
            .unwrap();
        io::stdout().write_all("\n".as_bytes()).unwrap();
        if env::var("CARGO_CFG_TARGET_ENV").unwrap().contains("musl") {
            // Static linking with musl
            io::stdout()
                .write_all("cargo:rustc-link-search=".as_bytes())
                .unwrap();
            io::stdout()
                .write_all(src_dir.join("libs").as_os_str().as_bytes())
                .unwrap();
            io::stdout()
                .write_all("\ncargo:rustc-link-lib=static=elf\n\
                          cargo:rustc-link-lib=static=z\n\
                          cargo:rustc-link-lib=static=argp\n\
                          cargo:rustc-link-lib=static=fts\n\
                          cargo:rustc-link-lib=static=obstack\n\
                          cargo:rustc-link-lib=static=bpf\n".as_bytes())
                .unwrap();
        } else {
            // Assume something else. Link dynamically (upstream behaviour)
            io::stdout()
                .write_all("\ncargo:rustc-link-lib=elf\n\
                          cargo:rustc-link-lib=z\n\
                          cargo:rustc-link-lib=static=bpf\n".as_bytes())
                .unwrap();
        }
        io::stdout().write_all("cargo:include=".as_bytes()).unwrap();
        io::stdout()
            .write_all(out_dir.as_os_str().as_bytes())
            .unwrap();
        io::stdout().write_all("/include\n".as_bytes()).unwrap();
    }
}
