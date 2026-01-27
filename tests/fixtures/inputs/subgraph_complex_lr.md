graph LR
API[API Gateway]
subgraph SG1 [Service Layer]
S1[User Service]
S2[Order Service]
S1 --> S2
end
subgraph SG2 [Data Layer]
D1[(User DB)]
D2[(Order DB)]
end
Response[Response Builder]
API --> S1
S1 --> D1
S2 --> D2
D1 --> Response
D2 --> Response
