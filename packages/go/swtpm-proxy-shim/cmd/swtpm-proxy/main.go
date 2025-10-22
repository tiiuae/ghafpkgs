// Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0
package main

import (
	"flag"
	"fmt"
	"log"
	"os"
	"strconv"

	swtpmproxy "github.com/abrandao-census/swtpm-proxy-shim"
)

func usage() {
	fmt.Fprintf(os.Stderr, "Usage: %s --type <vsock|tcp> --control-port <port> [--data-port <port>] [--control-retry-count <count>] <listen-socket> <host>\n", os.Args[0])
	os.Exit(1)
}

func main() {

	var options swtpmproxy.SwtpmProxyOptions

	connType := flag.String("type", "", "Connection type: vsock or tcp")
	controlPort := flag.Int("control-port", 0, "Control port number")
	dataPort := flag.Int("data-port", 0, "Data port number (optional)")
	controlRetryCount := flag.Int("control-retry-count", 10, "Control retry count (optional)")

	flag.Usage = usage
	flag.Parse()

	switch *connType {
	case "vsock":
		options.BackendType = swtpmproxy.BackendVsock
	case "tcp":
		options.BackendType = swtpmproxy.BackendIP
	default:
		fmt.Fprintf(os.Stderr, "Error: --type must be 'vsock' or 'tcp', got: %s\n", *connType)
		usage()
	}
	if *controlPort <= 0 || *controlPort > 65535 {
		fmt.Fprintln(os.Stderr, "Error: --control-port must be > 0 and <= 65535")
		usage()
	}
	options.BackendControlPort = uint16(*controlPort)

	if *dataPort == 0 {
		*dataPort = *controlPort + 1
	}
	if *dataPort > 65535 {
		fmt.Fprintln(os.Stderr, "Error: --data-port must be >= 0 and <= 65535")
		usage()
	}
	options.BackendDataPort = uint16(*dataPort)
	args := flag.Args()
	if len(args) != 2 {
		fmt.Fprintln(os.Stderr, "Error: Missing required positional arguments <listen-socket> and <host>")
		usage()
	}

	options.ControlSocketPath = args[0]
	switch options.BackendType {
	case swtpmproxy.BackendVsock:
		cid, err := strconv.Atoi(args[1])
		if err != nil {
			log.Fatalf("Invalid CID: %v", err)
		}
		options.BackendCid = uint32(cid)
	case swtpmproxy.BackendIP:
		options.BackendAddress = args[1]
	}
	options.BackendControlRetryCount = *controlRetryCount

	fmt.Printf("Flags: type=%s, control-port=%d, data-port=%d, control-retry-count=%d\n", *connType, *controlPort, *dataPort, *controlRetryCount)
	fmt.Printf("Control Socket Path: %s, Backend Address: %s Cid: %d\n", options.ControlSocketPath, options.BackendAddress, options.BackendCid)
	startProxy(options)
}

func startProxy(opts swtpmproxy.SwtpmProxyOptions) {

	proxy := swtpmproxy.NewSwtpmProxy(opts)

	err := proxy.Start()
	if err != nil {
		log.Fatalf("Failed to start proxy: %v", err)
	}
}
