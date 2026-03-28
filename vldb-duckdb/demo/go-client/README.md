# Go client demo

## 1) Generate protobuf stubs

```bash
go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.11
go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@v1.6.1
./generate.sh
```

## 2) Run the demo client

```bash
go run . -addr 127.0.0.1:50052 -out ./query.arrow.stream
```

The client will:

1. call `ExecuteScript` to create the demo table
2. insert rows with parameterized SQL via `params_json`
3. call `QueryJson` for a lightweight `count(*)` result
4. call `QueryStream` for Arrow IPC row retrieval
5. write the `.arrow.stream` file locally
6. open it again with Apache Arrow Go and print basic batch statistics
