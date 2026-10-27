graph BT
DS[(Task)] --> Z1[OutletOne]
HS{{Task}} --> Z2[OutletTwo]
QS{Task} --> Z3[OutletTri]
A1[InletOne] --> DT[(Task)]
A2[InletTwo] --> HT{{Task}}
A3[InletTri] --> QT{Task}
R0[Task]
