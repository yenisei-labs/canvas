# Canvas - Image processing server

A complete solution for storing and processing images uploaded by users.

Images are processed on the fly and cached for faster subsequent requests.

| Canvas | v0.0.13 |
| ---    | ---     |
| Status | Alpha   |

## Configuration

The server can be configured via environment variables. `.env` files are supported.

- `CANVAS_UPLOAD_DIR` - where to store uploaded photos? (for example: `/mnt/images`)
- `CANVAS_REDIS_URL` - url to Redis instance (for example: `redis://127.0.0.1:6379/`)
- `CANVAS_WATERMARK_FILE_PATH` - optional path to the image to be used as the watermark (for example: `/home/user/watermark.png`)
- `CANVAS_PORT` - optional port number (default: `3000`)

## Redis configuration

Processed photos will be saved to Redis to speed up recurring requests.

Don't forget to set appropriate policies for storage size.

Learn more:
- [Key eviction](https://redis.io/docs/reference/eviction/)

## Docker-compose example
```yml
version: '3.8'

services:
  image-cache:
    image: redis
    # Edit the parameters for your needs
    command: redis-server --maxmemory 100mb --maxmemory-policy allkeys-lru

  canvas:
    image: ghcr.io/yenisei-labs/canvas
    environment:
      CANVAS_UPLOAD_DIR: "/data"
      CANVAS_REDIS_URL: "redis://image-cache:6379/"
    volumes:
      - images:/data
    ports:
      - 3000:3000

volumes:
  images:
```

## API

- `POST /images` - upload new photo

Request:

```bash
curl -F 'image=@test.png' https://domain.tld/images
```

Response:

```json
{
    "hash": "string"
}
```

Error example:

```json
{
    "status_code": 400,
    "message": "What went wrong"
}
```

---

- `GET /images/<hash>` - get a photo

Optional query parameters:

- `width`: desired width (default: 1024px)
- `height`: desired height (default: 1024px)
- `quality`: image quality (1-100, default: 80)
- `watermark`: add a watermark? (true if the parameter is in the url, value doesn't matter)
- `format`: image format (supported values: `jpg` (or `jpeg`), `webp`, default: `webp`)
- `filename`: override the name of the returned file (default: hash.format)
- `overlay`: small text to be added to the top left corner, can be used instead of a watermark

Example:
```
GET https://domain.tld/images/IMAGE_HASH?width=300&height=300&quality=75&watermark=y&format=jpg&filename=photo.jpg
```

---

- `GET /health` - get server status

Responds with 200 OK if the server is running. At the moment, there is no additional information.

## Image processing steps

1. Apply rotation from exif tags.
2. Resize the image so that the smaller side fits completely into the specified dimensions.
3. Crop the image using a smart algorithm.
4. Apply a watermark if required.
5. Encode the photo in the required format, remove extra metadata.

The server does not change the aspect ratio.

If you specify a width and height, the resulting image will not necessarily be that size. The server does not upscale the photo.
