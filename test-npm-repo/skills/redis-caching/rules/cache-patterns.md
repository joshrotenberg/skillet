# Cache Patterns

## Cache-Aside

1. Check cache first
2. On miss, read from database
3. Populate cache with result
4. Return data

## Write-Through

1. Write to cache and database simultaneously
2. Ensures consistency at cost of latency
