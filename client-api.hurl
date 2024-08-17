POST http://localhost:8089/api/v1/write
Content-Type: application/json
{
    "namespace": "test",
    "message": "0x1234"
}
HTTP 200

GET http://localhost:8089/api/v1/read?namespace=test&start=0&end=1823846288000
Content-Type: application/json
HTTP 200