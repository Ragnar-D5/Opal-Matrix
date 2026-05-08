project('gst-mxc', 'c')

gst_dep = dependency('gstreamer-1.0')
gstbase_dep = dependency('gstreamer-base-1.0')

shared_module(
  'gstmxcuri',
  'gstmxcurisrc.c',
  dependencies: [gst_dep, gstbase_dep],
  install: true,
  install_dir: gst_dep.get_variable('pluginsdir')
)
