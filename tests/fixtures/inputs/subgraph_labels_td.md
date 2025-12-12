graph TD
subgraph SG1 [Auth Flow]
Login[Login Form]
Auth[Authenticate]
Login -->|submit| Auth
end
Dashboard[Dashboard]
Auth -->|success| Dashboard
