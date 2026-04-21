import { createApp } from "vue";
import App from "./App.vue";
import 'vuetify/styles';
import { createVuetify } from 'vuetify';
import '@mdi/font/css/materialdesignicons.css';

const vuetify = createVuetify({
  theme: {
    defaultTheme: 'dark',
  },
});

const app = createApp(App);
app.use(vuetify);
app.mount("#app");
