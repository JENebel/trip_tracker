default: serve

package:
    cd frontend && trunk build
    cd frontend && cp static/favicon.ico dist/favicon.ico

serve: package
    cargo build -r --bin server && sudo ./target/release/server