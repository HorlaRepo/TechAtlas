import { defineConfig } from '@hey-api/openapi-ts';

export default defineConfig({
  input: '../../contracts/openapi/openapi.yaml',
  output: process.env.OPENAPI_OUTPUT ?? 'src/generated',
  plugins: ['@hey-api/client-fetch'],
});
