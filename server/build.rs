fn main() {
    // Rerun the build script if the .proto file changes
    println!("cargo:rerun-if-changed=../gps_track.proto");
    
    // Compile the .proto files
    prost_build::compile_protos(&["../gps_track.proto"], &["../"]).unwrap();
}
