#!/bin/bash

# Initialize variables
client_port="8000"
registry_path="registry-200.txt"
latency_limit="400"

build=false

function usage() {
    echo "Usage: $0 -p <port> -r <registry_path> -l <latency_limit>"
    echo "  -p  Set the client API port (default: $client_port)"
    echo "  -r  Registry file location (default: $registry_path)"
    echo "  -l  Set the latency limit in milliseconds (default: $latency_limit)"
    echo "  -b  Build images (default: $build)"
    exit 1
}


# Parse options
while getopts "p:r:l:bh" opt; do
    case $opt in
        p) client_port=$OPTARG;;
        r) registry_path=$OPTARG;;
        l) latency_limit=$OPTARG;;
        b) build=true;;
        *) usage;;
    esac
done

# Output the input options for verification
echo "Client Port: $client_port"
echo "Registry location: $registry_path"
echo "Latency Limit: $latency_limit"

if [ "$build" = true ]; then
    echo "Building docker images in $(pwd)"
    docker build -t dato-validator -f Dockerfile.validator --load .
    docker build -t dato-client -f Dockerfile.client --load .
    
    exit 0
fi

echo "Creating dato-net network"
docker network create -d bridge dato-net || true

# Read all lines in the registry
while IFS=',' read -r index privkey pubkey
do
    instance="dato-validator-$index"
    echo "Starting $instance"

    docker run -d --network dato-net --name $instance -e RUST_LOG=debug --cap-add=NET_ADMIN dato-validator run --secret-key $privkey --port 8222

    rand_latency=$(( ( RANDOM % $latency_limit ) + 1 ))

    cmd="tc qdisc add dev eth0 root netem delay ${rand_latency}ms"

    echo "Executing command: $cmd"
    # Add latency to the validator instance
    docker exec $instance $cmd
done < "$registry_path"

echo ""
echo "Waiting 5 seconds for validators to start..."
sleep 5


echo "Starting dato-client"
docker run -d --network dato-net --name dato-client -p $client_port:$client_port -e RUST_LOG=trace dato-client --registry-path "/${registry_path}" --api-port $client_port

docker logs -f dato-client

# Clean up
echo "Cleaning up"
docker rm --force $(docker ps -a -q)