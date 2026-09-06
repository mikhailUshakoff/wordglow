source .env

curl https://ollama.com/api/generate \
  -H "Authorization: Bearer $OLLAMA_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-oss:120b",
    "prompt": "Explain Rust ownership in simple terms.",
    "stream": false
  }'
#curl https://ollama.com/api/chat \
#  -H "Authorization: Bearer $OLLAMA_API_KEY" \
#  -H "Content-Type: application/json" \
#  -d '{
#    "model": "gpt-oss:120b",
#    "messages": [
#      {
#        "role": "user",
#        "content": "Explain how Rust ownership works."
#      }
#    ],
#    "stream": false
#  }'