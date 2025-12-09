graph BT
A[Database] --> B[Service A]
A --> C[Service B]
B --> D[API Gateway]
C --> D
D --> E[Client]