#!/bin/bash

if ! command -v cargo &> /dev/null; then
    echo "Cargo not found. Installing Rust and Cargo..."
    curl https://sh.rustup.rs -sSf | sh
else
    echo "Cargo is already installed"
fi


