#ifndef PACKAGE
#define PACKAGE "gst-mxc"
#endif
#include <gst/gst.h>
#include <gst/base/gstpushsrc.h>
#include <string.h>

#define GST_TYPE_MXC_URI_SRC (gst_mxc_uri_src_get_type())
G_DECLARE_FINAL_TYPE(GstMxcUriSrc, gst_mxc_uri_src, GST, MXC_URI_SRC, GstPushSrc)

struct _GstMxcUriSrc {
  GstPushSrc parent;
  gchar *uri;
};

/* ---- URI handler ---- */

static GstURIType
mxc_uri_get_type(GType type) {
  return GST_URI_SRC;
}

static const gchar * const *
mxc_uri_get_protocols(GType type) {
  static const gchar *protocols[] = { "mxc", NULL };
  return protocols;
}

static gchar *
mxc_uri_get_uri(GstURIHandler *handler) {
  GstMxcUriSrc *self = (GstMxcUriSrc *)handler;
  return g_strdup(self->uri);
}

static gboolean
mxc_uri_set_uri(GstURIHandler *handler, const gchar *uri, GError **error) {
  GstMxcUriSrc *self = (GstMxcUriSrc *)handler;

  g_free(self->uri);
  self->uri = g_strdup(uri);

  /* Do nothing else — we just accept it */
  return TRUE;
}

static void
mxc_uri_handler_init(gpointer g_iface, gpointer iface_data) {
  GstURIHandlerInterface *iface = (GstURIHandlerInterface *)g_iface;
  iface->get_type = mxc_uri_get_type;
  iface->get_protocols = mxc_uri_get_protocols;
  iface->get_uri = mxc_uri_get_uri;
  iface->set_uri = mxc_uri_set_uri;
}

/* ---- PushSrc implementation ---- */

static GstFlowReturn
gst_mxc_uri_src_create(GstPushSrc *src, GstBuffer **buf) {
  /* Immediately signal EOS — no data */
  return GST_FLOW_EOS;
}

static void
gst_mxc_uri_src_class_init(GstMxcUriSrcClass *klass) {
  GstPushSrcClass *pushsrc_class = GST_PUSH_SRC_CLASS(klass);
  pushsrc_class->create = gst_mxc_uri_src_create;

  gst_element_class_set_static_metadata(
    GST_ELEMENT_CLASS(klass),
    "MXC URI source",
    "Source",
    "Dummy handler for mxc:// URIs",
    "you"
  );

  /* minimal src pad */
  GstCaps *caps = gst_caps_new_any();
  gst_element_class_add_static_pad_template(
    GST_ELEMENT_CLASS(klass),
    & (GstStaticPadTemplate) {
      .name_template = "src",
      .direction = GST_PAD_SRC,
      .presence = GST_PAD_ALWAYS,
      .static_caps = GST_STATIC_CAPS_ANY
    }
  );
  gst_caps_unref(caps);
}

static void
gst_mxc_uri_src_init(GstMxcUriSrc *self) {
  self->uri = NULL;
}

/* ---- Plugin init ---- */

static gboolean
plugin_init(GstPlugin *plugin) {
  return gst_element_register(
    plugin,
    "mxcurisrc",
    GST_RANK_NONE,
    GST_TYPE_MXC_URI_SRC
  );
}

GST_PLUGIN_DEFINE(
  GST_VERSION_MAJOR,
  GST_VERSION_MINOR,
  mxcuri,
  "MXC URI handler",
  plugin_init,
  "1.0",
  "LGPL",
  "example",
  "example"
)
G_DEFINE_TYPE_WITH_CODE(
  GstMxcUriSrc,
  gst_mxc_uri_src,
  GST_TYPE_PUSH_SRC,
  G_IMPLEMENT_INTERFACE(GST_TYPE_URI_HANDLER, mxc_uri_handler_init)
)
