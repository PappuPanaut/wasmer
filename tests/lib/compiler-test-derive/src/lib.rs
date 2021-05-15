#[cfg(not(test))]
extern crate proc_macro;
#[cfg(not(test))]
use ::proc_macro::TokenStream;
#[cfg(test)]
use ::proc_macro2::TokenStream;
use ::quote::quote;
#[cfg(not(test))]
use ::syn::parse;
#[cfg(test)]
use ::syn::parse2 as parse;
// use std::iter::FromIterator;
use ::syn::*;
use std::path::PathBuf;

mod ignores;

// Reimplement parse_macro_input to use the imported `parse`
// function. This way parse_macro_input will parse a TokenStream2 when
// unit-testing.
macro_rules! parse_macro_input {
    (
        $token_stream:ident as $T:ty
    ) => {
        match parse::<$T>($token_stream) {
            Ok(data) => data,
            Err(err) => {
                return TokenStream::from(err.to_compile_error());
            }
        }
    };

    (
        $token_stream:ident
    ) => {
        parse_macro_input!($token_stream as _)
    };
}

#[proc_macro_attribute]
pub fn compiler_test(attrs: TokenStream, input: TokenStream) -> TokenStream {
    let path: Option<ExprPath> = parse::<ExprPath>(attrs).ok();
    let my_fn: ItemFn = parse_macro_input!(input as ItemFn);
    let fn_name = my_fn.sig.ident.clone();

    // Let's build the ignores to append an `#[ignore]` macro to the
    // autogenerated tests in case the test appears in the `ignores.txt` path;

    let mut ignores_txt_path = PathBuf::new();
    ignores_txt_path.push(env!("CARGO_MANIFEST_DIR"));
    ignores_txt_path.push("../../ignores.txt");

    let ignores = crate::ignores::Ignores::build_from_path(ignores_txt_path);

    let should_ignore = |test_name: &str, compiler_name: &str, engine_name: &str| {
        let compiler_name = compiler_name.to_lowercase();
        let engine_name = engine_name.to_lowercase();
        // We construct the path manually because we can't get the
        // source_file location from the `Span` (it's only available in nightly)
        let full_path = format!(
            "{}::{}::{}::{}",
            quote! { #path },
            test_name,
            compiler_name,
            engine_name
        )
        .replace(" ", "");
        let should_ignore = ignores.should_ignore_host(&engine_name, &compiler_name, &full_path);
        // println!("{} -> Should ignore: {}", full_path, should_ignore);
        return should_ignore;
    };
    let construct_engine_test = |sig: &::syn::Signature,
                                 compiler_name: &str,
                                 engine_name: &str|
     -> ::proc_macro2::TokenStream {
        let config_compiler = ::quote::format_ident!("{}", compiler_name);
        let config_engine = ::quote::format_ident!("{}", engine_name);
        let engine_name_lowercase = engine_name.to_lowercase();
        let test_name = ::quote::format_ident!("{}", engine_name_lowercase);
        let mut new_sig = sig.clone();
        new_sig.ident = test_name;
        new_sig.inputs = ::syn::punctuated::Punctuated::new();
        let func = quote! {
            #[test]
            #[cfg(feature = #engine_name_lowercase)]
            #new_sig {
                #fn_name(crate::Config::new(crate::Engine::#config_engine, crate::Compiler::#config_compiler))
            }
        };
        if should_ignore(
            &sig.ident.to_string().replace("r#", ""),
            compiler_name,
            engine_name,
        ) {
            quote! {
                #[ignore]
                #func
            }
        } else {
            func
        }
    };

    let construct_compiler_test =
        |sig: &::syn::Signature, compiler_name: &str| -> ::proc_macro2::TokenStream {
            let mod_name = ::quote::format_ident!("{}", compiler_name.to_lowercase());
            let jit_engine_test = construct_engine_test(sig, compiler_name, "JIT");
            let native_engine_test = construct_engine_test(sig, compiler_name, "Native");
            let compiler_name_lowercase = compiler_name.to_lowercase();

            quote! {
                #[cfg(feature = #compiler_name_lowercase)]
                mod #mod_name {
                    use super::*;

                    #jit_engine_test
                    #native_engine_test
                }
            }
        };

    let singlepass_compiler_test = construct_compiler_test(&my_fn.sig, "Singlepass");
    let cranelift_compiler_test = construct_compiler_test(&my_fn.sig, "Cranelift");
    let llvm_compiler_test = construct_compiler_test(&my_fn.sig, "LLVM");

    let x = quote! {
        #[cfg(test)]
        mod #fn_name {
            use super::*;

            #my_fn

            #singlepass_compiler_test
            #cranelift_compiler_test
            #llvm_compiler_test
        }
    };
    x.into()
}

#[cfg(test)]
mod tests;
