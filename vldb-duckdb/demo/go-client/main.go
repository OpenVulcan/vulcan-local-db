package main

import (
	"bytes"
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"log"
	"os"
	"time"

	"github.com/apache/arrow-go/v18/arrow/ipc"
	duckdbv1 "vldb-duckdb-go-demo/proto/v1"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

type demoRow struct {
	ID     int32
	Name   string
	Active bool
}

func main() {
	addr := flag.String("addr", "127.0.0.1:50052", "vldb-duckdb gRPC endpoint")
	out := flag.String("out", "query.arrow.stream", "output Arrow IPC stream file")
	timeout := flag.Duration("timeout", 30*time.Second, "request timeout")
	flag.Parse()

	ctx, cancel := context.WithTimeout(context.Background(), *timeout)
	defer cancel()

	conn, err := grpc.DialContext(
		ctx,
		*addr,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
		grpc.WithBlock(),
	)
	if err != nil {
		log.Fatalf("dial gRPC server failed: %v", err)
	}
	defer conn.Close()

	client := duckdbv1.NewDuckDbServiceClient(conn)
	setupResp, err := client.ExecuteScript(ctx, &duckdbv1.ExecuteRequest{
		Sql: `
drop table if exists demo_items;
create table demo_items(id integer, name varchar, active boolean);
`,
	})
	if err != nil {
		log.Fatalf("ExecuteScript setup failed: %v", err)
	}
	log.Printf("ExecuteScript setup => success=%v message=%s", setupResp.Success, setupResp.Message)

	rows := []demoRow{
		{ID: 1, Name: "alpha", Active: true},
		{ID: 2, Name: "beta'); drop table demo_items; --", Active: true},
		{ID: 3, Name: "gamma", Active: false},
	}
	insertSQL := "insert into demo_items(id, name, active) values (?, ?, ?)"
	for _, row := range rows {
		paramsJSON, err := json.Marshal([]any{row.ID, row.Name, row.Active})
		if err != nil {
			log.Fatalf("marshal insert params failed: %v", err)
		}

		insertResp, err := client.ExecuteScript(ctx, &duckdbv1.ExecuteRequest{
			Sql:        insertSQL,
			ParamsJson: string(paramsJSON),
		})
		if err != nil {
			log.Fatalf("ExecuteScript insert failed: %v", err)
		}
		log.Printf("ExecuteScript insert => id=%d message=%s", row.ID, insertResp.Message)
	}

	countParamsJSON, err := json.Marshal([]any{true})
	if err != nil {
		log.Fatalf("marshal QueryJson params failed: %v", err)
	}

	queryJSONResp, err := client.QueryJson(ctx, &duckdbv1.QueryRequest{
		Sql:        "select count(*) as total_active from demo_items where active = ?",
		ParamsJson: string(countParamsJSON),
	})
	if err != nil {
		log.Fatalf("QueryJson RPC failed: %v", err)
	}
	log.Printf("QueryJson => %s", queryJSONResp.JsonData)

	stream, err := client.QueryStream(ctx, &duckdbv1.QueryRequest{
		Sql:        "select id, name, active from demo_items where active = ? order by id",
		ParamsJson: string(countParamsJSON),
	})
	if err != nil {
		log.Fatalf("QueryStream RPC failed: %v", err)
	}

	var payload bytes.Buffer
	for {
		msg, err := stream.Recv()
		if err == io.EOF {
			break
		}
		if err != nil {
			log.Fatalf("receive stream chunk failed: %v", err)
		}
		if _, err := payload.Write(msg.GetArrowIpcChunk()); err != nil {
			log.Fatalf("buffer Arrow chunk failed: %v", err)
		}
	}

	if err := os.WriteFile(*out, payload.Bytes(), 0o644); err != nil {
		log.Fatalf("write Arrow IPC stream file failed: %v", err)
	}

	reader, err := ipc.NewReader(bytes.NewReader(payload.Bytes()))
	if err != nil {
		log.Fatalf("open Arrow IPC reader failed: %v", err)
	}
	defer reader.Release()

	fmt.Printf("schema: %s\n", reader.Schema())

	totalRows := int64(0)
	batchIndex := 0
	for reader.Next() {
		rec := reader.Record()
		fmt.Printf("batch[%d]: rows=%d cols=%d\n", batchIndex, rec.NumRows(), rec.NumCols())
		totalRows += int64(rec.NumRows())
		batchIndex++
	}

	if err := reader.Err(); err != nil {
		log.Fatalf("read Arrow IPC batches failed: %v", err)
	}

	fmt.Printf("saved Arrow stream to %s, total_rows=%d, batches=%d\n", *out, totalRows, batchIndex)
}
