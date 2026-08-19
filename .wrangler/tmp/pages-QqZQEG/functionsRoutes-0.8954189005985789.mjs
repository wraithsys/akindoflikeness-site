import { onRequestPost as __api_notify_js_onRequestPost } from "/home/user/akindoflikeness-site/functions/api/notify.js"

export const routes = [
    {
      routePath: "/api/notify",
      mountPath: "/api",
      method: "POST",
      middlewares: [],
      modules: [__api_notify_js_onRequestPost],
    },
  ]