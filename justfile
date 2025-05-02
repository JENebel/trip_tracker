default: serve

package:
    cd frontend && trunk build

serve: package
    cargo build -r --bin server && sudo ./target/release/server