name: Feature Request
description: Suggest a new feature or improvement
title: "[Feature]: "
labels: ["enhancement", "triage"]
assignees: []
body:
  - type: markdown
    attributes:
      value: |
        Thanks for suggesting a feature! Please provide as much detail as possible.

  - type: textarea
    id: problem
    attributes:
      label: Problem Statement
      description: What problem does this feature solve?
      placeholder: I'm frustrated when... This would help users...
    validations:
      required: true

  - type: textarea
    id: solution
    attributes:
      label: Proposed Solution
      description: Describe the solution you'd like
      placeholder: |
        Add a new crate for X...
        Implement Y in the existing Z module...
    validations:
      required: true

  - type: textarea
    id: alternatives
    attributes:
      label: Alternative Solutions
      description: What alternatives have you considered?
      placeholder: We could also try... but this approach is better because...
    validations:
      required: false

  - type: dropdown
    id: phase
    attributes:
      label: Related Phase
      description: Which phase of the roadmap does this relate to?
      options:
        - Phase 1: Account seed + getAccountInfo (✅ Complete)
        - Phase 2: Geyser delta (🚧 In Progress)
        - Phase 3: Program + token indexes (📋 Planned)
        - Phase 4: getProgramAccounts planner (📋 Planned)
        - Phase 5: Materialized query engine (📋 Planned)
        - Not related to current roadmap
    validations:
      required: true

  - type: textarea
    id: implementation
    attributes:
      label: Implementation Details
      description: Any specific implementation details or API design ideas
      placeholder: |
        ```rust
        pub struct NewFeature {
            // ...
        }
        ```
      render: rust
    validations:
      required: false

  - type: textarea
    id: additional
    attributes:
      label: Additional Context
      description: Any other context, examples, or references
    validations:
      required: false
