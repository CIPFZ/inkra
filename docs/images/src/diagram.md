# Diagrams

Mermaid flowcharts, sequence diagrams and pie charts are drawn natively.

```mermaid
flowchart LR
  A[Write] --> B{Ready?}
  B -->|yes| C[Export PDF]
  B -->|no| D[Revise]
  D --> A
```

```mermaid
pie title Time spent
  "Writing" : 55
  "Editing" : 30
  "Formatting" : 15
```
