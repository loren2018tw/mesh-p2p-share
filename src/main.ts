import { createApp } from "vue";
import App from "./App.vue";
import "vuetify/styles";
import { createVuetify } from "vuetify";
import "@mdi/font/css/materialdesignicons.css";

const vuetify = createVuetify({
  theme: {
    defaultTheme: "classical",
    themes: {
      classical: {
        dark: false,
        colors: {
          background: "#EDE3CE",
          surface: "#F8F2E4",
          primary: "#7B5535",
          secondary: "#5B7850",
          error: "#8B3B3B",
          warning: "#9A7030",
          info: "#486882",
          success: "#4A7050",
        },
      },
    },
  },
});

const app = createApp(App);
app.use(vuetify);
app.mount("#app");
