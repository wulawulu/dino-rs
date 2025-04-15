use proc_macro::TokenStream;

mod process;

use process::*;

#[proc_macro_derive(FromJs)]
pub fn derive_enum_from(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    process_from_js(input).into()
}

#[proc_macro_derive(IntoJs)]
pub fn derive_enum_from_darling(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    process_into_js(input).into()
}
