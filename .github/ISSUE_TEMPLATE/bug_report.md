name: Bug Report
description: Report a bug to help us improve
title: "[Bug]: "
labels: ["bug", "triage"]
assignees: []
body:
  - type: markdown
    attributes:
      value: |
        Thanks for taking the time to fill out this bug report!

  - type: textarea
    id: description
    attributes:
      label: Description
      description: Briefly describe the bug
      placeholder: What happened? What did you expect to happen?
    validations:
      required: true

  - type: textarea
    id: reproduction
    attributes:
      label: Steps to Reproduce
      description: Step-by-step instructions to reproduce the bug
      placeholder: |
        1. Build with `cargo build --workspace --features redb-backend`
        2. Run `./target/release/synapse-base-scanner --accounts-path ...`
        3. See error...
    validations:
      required: true

  - type: textarea
    id: expected
    attributes:
      label: Expected Behavior
      description: What should happen instead
      placeholder: The scanner should process all accounts without errors
    validations:
      required: true

  - type: input
    id: os
    attributes:
      label: Operating System
      description: What OS are you using?
      placeholder: macOS 15.5 / Ubuntu 24.04
    validations:
      required: true

  - type: input
    id: rust-version
    attributes:
      label: Rust Version
      description: Output of `rustc --version`
      placeholder: rustc 1.85.0 (4d91de4e4 2025-02-17)
    validations:
      required: true

  - type: dropdown
    id: backend
    attributes:
      label: Storage Backend
      description: Which storage backend are you using?
      options:
        - redb (default, macOS)
        - RocksDB (Linux production)
    validations:
      required: true

  - type: textarea
    id: logs
    attributes:
      label: Error Logs
      description: Paste any relevant error logs or stack traces
      placeholder: |
        ```
        thread 'main' panicked at ...
        ```
      render: shell
    validations:
      required: false

  - type: textarea
    id: additional
    attributes:
      label: Additional Context
      description: Any other details, screenshots, or related issues
    validations:
      required: false
