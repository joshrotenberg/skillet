# Embedding Guide

## Choosing an Embedding Model

- OpenAI text-embedding-3-small: good balance of cost and quality
- Cohere embed-v3: multilingual support
- Local models: sentence-transformers for privacy

## Indexing Strategy

1. Choose vector dimensions matching your model
2. Use HNSW for approximate nearest neighbor search
3. Set M and EF_CONSTRUCTION parameters based on dataset size
