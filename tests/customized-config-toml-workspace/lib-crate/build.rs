fn main() {
    // Exercise wdk-build metadata path respecting by invoking the library.
    // We only need to call a function that triggers cargo_metadata under the hood.
    wdk_build::configure_wdk_library_build()
        .expect("configure_wdk_library_build should run successfully in this test");
}
