POST http://localhost:8089/api/v1/write
Content-Type: application/json
{
    "namespace": "test",
    "message": "0x1234"
}
HTTP 200

GET http://localhost:8089/api/v1/read?namespace=test&start=0&end=1823846288000
HTTP 200

GET http://localhost:8089/api/v1/read_message?namespace=test&msg_id=0x154b77cb48d6dd752411023a48b3a704009e0259116a0ee1350cc8afb5d51d12
HTTP 200
