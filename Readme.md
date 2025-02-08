This repository contains several crates.

- The embedded tracker code for the Esp32-S3 / LilyGo T-Sim7670
- The frontend web interface powered by Yew and Leaflet
- The central server that receives tracker data and serves the web frontend
- A data management library that controls tracker data buffering and the SQLite database
  - A standalone CLI to modify the data on the server manually is provided
- A general library with data types and other shared functionality across the crates

To run the server:

    just serve


To get ready from zero. Automate at some point?

    ** Install rust and cargo with rustup **
    ** Create a file; frontend/maptiler_key.txt containing the MapTiler API key

    rustup target add wasm32-unknown-unknown

    sudo apt install just
    cargo install espup
    espup install