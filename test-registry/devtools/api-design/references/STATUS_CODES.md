# HTTP Status Code Guide

## 2xx Success

| Code | Meaning | When to use |
|------|---------|-------------|
| 200  | OK | Successful GET, PUT, PATCH, DELETE |
| 201  | Created | Successful POST that created a resource |
| 202  | Accepted | Request accepted for async processing |
| 204  | No Content | Successful DELETE with no response body |

## 3xx Redirection

| Code | Meaning | When to use |
|------|---------|-------------|
| 301  | Moved Permanently | Resource permanently moved |
| 304  | Not Modified | Cache is still valid (ETag/If-Modified-Since) |

## 4xx Client Errors

| Code | Meaning | When to use |
|------|---------|-------------|
| 400  | Bad Request | Malformed syntax, invalid parameters |
| 401  | Unauthorized | Missing or invalid authentication |
| 403  | Forbidden | Authenticated but not authorized |
| 404  | Not Found | Resource doesn't exist |
| 409  | Conflict | State conflict (duplicate, version mismatch) |
| 422  | Unprocessable Entity | Valid syntax but semantic errors |
| 429  | Too Many Requests | Rate limit exceeded |

## 5xx Server Errors

| Code | Meaning | When to use |
|------|---------|-------------|
| 500  | Internal Server Error | Unexpected server failure |
| 502  | Bad Gateway | Upstream service failure |
| 503  | Service Unavailable | Temporarily overloaded or in maintenance |
| 504  | Gateway Timeout | Upstream service timeout |
