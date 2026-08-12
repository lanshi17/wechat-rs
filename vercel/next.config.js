/** @type {import('next').NextConfig} */
const nextConfig = {
  // Serve admin HTML at root
  async rewrites() {
    return [
      // Proxy all backend API calls to the Rust backend
      {
        source: '/admin/:path*',
        destination: `${process.env.BACKEND_URL}/admin/:path*`,
      },
      {
        source: '/wx',
        destination: `${process.env.BACKEND_URL}/wx`,
      },
      {
        source: '/users',
        destination: `${process.env.BACKEND_URL}/users`,
      },
      {
        source: '/api/wechat/:path*',
        destination: `${process.env.BACKEND_URL}/api/wechat/:path*`,
      },
      {
        source: '/api/monitor/:path*',
        destination: `${process.env.BACKEND_URL}/api/monitor/:path*`,
      },
    ];
  },
};

module.exports = nextConfig;
