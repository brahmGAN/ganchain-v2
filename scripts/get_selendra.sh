#!/bin/bash
#
# Future Work: Implement friendly creating systemd service with user-input, features "--base-path, --name"

version=("0.2.1")

## Fetch gpu Binary
wget https://github.com/gpu/gpu/releases/download/$version/gpu

## Make gpu binary executable
chmod +x gpu

## copy gpu systemd service to systemd directory
sudo cp ./packaging/gpu.service /etc/systemd/system/gpu.service

## Enable gpu service
sudo systemctl enable gpu.service


