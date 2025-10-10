BINARY_NAME = swtpm-proxy
CMD_DIR = cmd
BIN_DIR = bin

.PHONY: all build run clean

all: build

build:
	mkdir -p $(BIN_DIR)
	go build -o $(BIN_DIR)/$(BINARY_NAME) $(CMD_DIR)/swtpm-proxy/main.go

run: build
	./$(BIN_DIR)/$(BINARY_NAME)

clean:
	rm -f $(BIN_DIR)/$(BINARY_NAME)
