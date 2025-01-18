default: serve

package:
    cd frontend && trunk build

serve: package
    cargo run -r --bin server