// Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0
package swtpmproxy

import (
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"os"
	"sync"
	"syscall"
	"time"

	"github.com/mdlayher/vsock"
)

const (
	CMD_SET_DATAFD = 0x10
	TPM_SUCCESS    = 0
	TPM_FAIL       = 9
)

type BackendType int

const (
	BackendIP BackendType = iota
	BackendVsock
)

type BackendChannel struct {
	port uint16
}

type ConnChannelProxy struct {
	backendConn io.ReadWriteCloser
	qemuConn    io.ReadWriteCloser
}

type TpmProxyChannels struct {
	controlChannel ConnChannelProxy
	dataChannel    ConnChannelProxy
}

type SwtpmProxyOptions struct {
	ControlSocketPath string // Path to the UNIX socket to listen on

	BackendType    BackendType // Type of backend connection (IP or Vsock)
	BackendAddress string      // Remote address to connect to
	BackendCid     uint32      // CID for vsock connections

	BackendControlPort uint16 // Port for backend control connection
	BackendDataPort    uint16 // Port for backend data connection

	BackendControlRetryCount int // Number of retries for backend control connection
}

type SwtpmProxy struct {
	Options SwtpmProxyOptions
}

func NewSwtpmProxy(options SwtpmProxyOptions) *SwtpmProxy {
	return &SwtpmProxy{
		Options: options,
	}
}

func isSetDataFdCmd(buf []byte) error {

	if len(buf) < 4 {
		return fmt.Errorf("buffer less than 4 bytes long")
	}
	if binary.BigEndian.Uint32(buf) != CMD_SET_DATAFD {
		return fmt.Errorf("not a CMD_SET_DATAFD command, expected 0x10, got %x", binary.BigEndian.Uint32(buf))
	}
	return nil
}

func parseSetDataOob(buf []byte) (uint32, error) {
	if len(buf) == 0 {
		return 0, fmt.Errorf("no ancillary data received")
	}
	scmss, err := syscall.ParseSocketControlMessage(buf)
	if err != nil {
		return 0, fmt.Errorf("failed to parse socket control message: %w", err)
	}
	if len(scmss) != 1 {
		return 0, fmt.Errorf("expected exactly one socket control message, got: %d", len(scmss))
	}
	scmVal := scmss[0]
	if scmVal.Header.Type != syscall.SCM_RIGHTS || scmVal.Header.Level != syscall.SOL_SOCKET {
		return 0, fmt.Errorf("expected SCM_RIGHTS socket control message, got: type = %x , level = %x", scmVal.Header.Type, scmVal.Header.Level)
	}
	if scmVal.Header.Len < 4 {
		return 0, fmt.Errorf("socket control message length too short: %d", scmVal.Header.Len)
	}
	return binary.NativeEndian.Uint32(scmVal.Data[:4]), nil
}

func (p *SwtpmProxy) handleQemuSetFd(backendControl io.ReadWriteCloser, qemuControl *net.UnixConn) (*TpmProxyChannels, error) {
	const bufferSize = 4096
	buf := make([]byte, bufferSize)
	oob := make([]byte, bufferSize)
	tpmResult := TPM_FAIL

	defer func() {
		res := make([]byte, 0, 4)
		res = binary.BigEndian.AppendUint32(res, uint32(tpmResult))
		qemuControl.Write(res)
	}()

	n, oobn, _, _, err := qemuControl.ReadMsgUnix(buf, oob)

	if err != nil {
		return nil, fmt.Errorf("failed to read from QEMU control socket: %w", err)
	}

	// The first message sent by QEMU is the setfd command, if not we fail
	if err = isSetDataFdCmd(buf[:n]); err != nil {
		return nil, err
	}

	fd, err := parseSetDataOob(oob[:oobn])
	if err != nil {
		return nil, err
	}

	qemuDataChan := os.NewFile(uintptr(fd), "data_fd")
	if qemuDataChan == nil {
		return nil, fmt.Errorf("received an invalid file descriptor from qemu: %d", fd)
	}

	backendDataConn, err := p.dialBackend(BackendChannel{p.Options.BackendDataPort})

	if err != nil {
		qemuDataChan.Close()
		return nil, fmt.Errorf("failed dialing to backend data channel: %w", err)
	}

	tpmResult = TPM_SUCCESS
	return &TpmProxyChannels{
		controlChannel: ConnChannelProxy{
			backendConn: backendControl,
			qemuConn:    qemuControl,
		},
		dataChannel: ConnChannelProxy{
			backendConn: backendDataConn,
			qemuConn:    qemuDataChan,
		},
	}, nil
}

func (p *SwtpmProxy) dialBackend(channel BackendChannel) (io.ReadWriteCloser, error) {
	var backendConn io.ReadWriteCloser
	var err error

	switch p.Options.BackendType {
	case BackendIP:
		backendConn, err = net.Dial("tcp", net.JoinHostPort(p.Options.BackendAddress, fmt.Sprintf("%d", channel.port)))
	case BackendVsock:
		backendConn, err = vsock.Dial(p.Options.BackendCid, uint32(channel.port), nil)
	default:
		err = fmt.Errorf("unsupported backend type '%d' specified", p.Options.BackendType)
	}

	if err != nil {
		return nil, fmt.Errorf("failed dialing to backend: %w", err)
	}

	return backendConn, nil
}

func (p *SwtpmProxy) dialBackendWithRetry(channel BackendChannel, maxRetries int, retryDelay time.Duration) (io.ReadWriteCloser, error) {
	var lastErr error

	for attempt := 0; attempt < maxRetries; attempt++ {
		if attempt > 0 {
			fmt.Printf("Retrying backend connection (attempt %d/%d) in %v...\n", attempt+1, maxRetries, retryDelay)
			time.Sleep(retryDelay)
		}

		conn, err := p.dialBackend(channel)
		if err == nil {
			if attempt > 0 {
				fmt.Printf("Successfully connected to backend after %d retries\n", attempt)
			}
			return conn, nil
		}

		lastErr = err
	}

	return nil, fmt.Errorf("failed to connect to backend after %d attempts: %w", maxRetries, lastErr)
}

func (p *SwtpmProxy) Start() error {
	if _, err := os.Stat(p.Options.ControlSocketPath); err == nil {
		err := os.Remove(p.Options.ControlSocketPath)
		if err != nil {
			return fmt.Errorf("failed to remove existing socket file %s: %w", p.Options.ControlSocketPath, err)
		}
	}

	l, err := net.Listen("unix", p.Options.ControlSocketPath)

	if err != nil {
		return fmt.Errorf("failed to listen on unix socket %s: %w", p.Options.ControlSocketPath, err)
	}

	defer l.Close()

	for {
		clientConn, err := l.Accept()
		if err != nil {
			return fmt.Errorf("accept error: %w", err)
		}

		qemuControlConn, ok := clientConn.(*net.UnixConn)
		if !ok {
			clientConn.Close()
			continue
		}

		// Retry control channel connection for TPM-VM boot delay
		fmt.Println("Connecting to backend control channel...")
		swtpmControlConn, err := p.dialBackendWithRetry(BackendChannel{p.Options.BackendControlPort}, p.Options.BackendControlRetryCount, 2*time.Second)

		if err != nil {
			clientConn.Close()
			return fmt.Errorf("failed dialing to backend control channel after retries: %w", err)
		}
		fmt.Println("New connection established, handling QEMU setfd command...")

		chans, err := p.handleQemuSetFd(swtpmControlConn, qemuControlConn)

		if err != nil {
			fmt.Printf("Error handling QEMU setfd command: %v\n", err)
			swtpmControlConn.Close()
			qemuControlConn.Close()
		}

		fmt.Println("setfd parsed successfully")
		fmt.Println("Starting proxy channels...")

		// We don't want to handle multiple connections for the same vTPM instance, run this synchronously
		err = chans.Proxy()
		if err != nil {
			fmt.Printf("Error during proxying: %v\n", err)
		}
	}

}

func (p *TpmProxyChannels) Proxy() error {
	errCh := make(chan error, 4)

	proxy := func(dst io.WriteCloser, src io.ReadCloser) {
		_, err := io.Copy(dst, src)
		errCh <- err
	}

	var wg sync.WaitGroup
	wg.Go(func() { proxy(p.controlChannel.backendConn, p.controlChannel.qemuConn) })
	wg.Go(func() { proxy(p.controlChannel.qemuConn, p.controlChannel.backendConn) })
	wg.Go(func() { proxy(p.dataChannel.backendConn, p.dataChannel.qemuConn) })
	wg.Go(func() { proxy(p.dataChannel.qemuConn, p.dataChannel.backendConn) })

	err := <-errCh

	p.controlChannel.backendConn.Close()
	p.controlChannel.qemuConn.Close()
	p.dataChannel.backendConn.Close()
	p.dataChannel.qemuConn.Close()

	wg.Wait()

	return err
}
