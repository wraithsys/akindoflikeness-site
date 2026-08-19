// <define:__ROUTES__>
var define_ROUTES_default = {
  version: 1,
  include: ["/api/*"],
  exclude: []
};

// ../../../tmp/claude-0/-home-user/e585398d-7fe0-5ae8-bcf4-eed0b42a6fcb/scratchpad/wtest/node_modules/wrangler/templates/pages-dev-pipeline.ts
import worker from "/home/user/akindoflikeness-site/.wrangler/tmp/pages-QqZQEG/functionsWorker-0.19151960902014387.mjs";
import { isRoutingRuleMatch } from "/tmp/claude-0/-home-user/e585398d-7fe0-5ae8-bcf4-eed0b42a6fcb/scratchpad/wtest/node_modules/wrangler/templates/pages-dev-util.ts";
export * from "/home/user/akindoflikeness-site/.wrangler/tmp/pages-QqZQEG/functionsWorker-0.19151960902014387.mjs";
var routes = define_ROUTES_default;
var pages_dev_pipeline_default = {
  fetch(request, env, context) {
    const { pathname } = new URL(request.url);
    for (const exclude of routes.exclude) {
      if (isRoutingRuleMatch(pathname, exclude)) {
        return env.ASSETS.fetch(request);
      }
    }
    for (const include of routes.include) {
      if (isRoutingRuleMatch(pathname, include)) {
        const workerAsHandler = worker;
        if (workerAsHandler.fetch === void 0) {
          throw new TypeError("Entry point missing `fetch` handler");
        }
        return workerAsHandler.fetch(request, env, context);
      }
    }
    return env.ASSETS.fetch(request);
  }
};
export {
  pages_dev_pipeline_default as default
};
//# sourceMappingURL=vkcna01iu7p.js.map
